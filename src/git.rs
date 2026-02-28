use async_trait::async_trait;
use corvia_common::errors::{CorviaError, Result};
use corvia_common::types::KnowledgeEntry;
use corvia_kernel::traits::IngestionAdapter;
use git2::Repository;
use std::path::Path;
use tracing::{info, debug, warn};
use walkdir::WalkDir;

use crate::treesitter::{self, CodeRelation};

/// Result of relation-aware ingestion: knowledge entries plus structural relations.
pub struct IngestionResult {
    pub entries: Vec<KnowledgeEntry>,
    pub relations: Vec<CodeRelation>,
}

pub struct GitAdapter;

impl GitAdapter {
    pub fn new() -> Self {
        Self
    }
}

/// File extensions we attempt to parse.
const SUPPORTED_EXTENSIONS: &[&str] = &["rs", "js", "jsx", "ts", "tsx", "py", "md", "toml", "yaml", "yml", "json"];

/// Directories to skip during ingestion.
const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".corvia", "dist", "build", "__pycache__", ".venv", "vendor"];

#[async_trait]
impl IngestionAdapter for GitAdapter {
    fn domain(&self) -> &str {
        "git"
    }

    async fn ingest(&self, source_path: &str) -> Result<Vec<KnowledgeEntry>> {
        let result = self.ingest_with_relations(source_path).await?;
        Ok(result.entries)
    }
}

impl GitAdapter {
    /// Ingest a source directory and return both knowledge entries and structural relations.
    ///
    /// Unlike `ingest()` (from the IngestionAdapter trait), this method also extracts
    /// structural relations (imports, implements, contains) from the AST. The
    /// `from_chunk_index` in each CodeRelation is offset to be globally unique across
    /// all files (matching the index into the entries vec).
    pub async fn ingest_with_relations(&self, source_path: &str) -> Result<IngestionResult> {
        let path = Path::new(source_path);
        if !path.exists() {
            return Err(CorviaError::Ingestion(format!(
                "Path does not exist: {source_path}"
            )));
        }

        let source_version = get_head_sha(path).unwrap_or_else(|| "unknown".to_string());
        let scope_id = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        info!(
            "Ingesting with relations {} (version: {}, scope: {})",
            source_path, source_version, scope_id
        );

        let mut entries = Vec::new();
        let mut all_relations = Vec::new();

        for entry in WalkDir::new(path).into_iter().filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !SKIP_DIRS.iter().any(|skip| name == *skip)
        }) {
            let entry = entry.map_err(|e| CorviaError::Ingestion(format!("Walk error: {e}")))?;
            if !entry.file_type().is_file() {
                continue;
            }

            let file_path = entry.path();
            let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if !SUPPORTED_EXTENSIONS.contains(&extension) {
                continue;
            }

            let Ok(source) = std::fs::read_to_string(file_path) else {
                debug!(
                    "Skipping binary or unreadable file: {}",
                    file_path.display()
                );
                continue;
            };

            if source.len() > 100_000 {
                warn!(
                    "Skipping large file ({}KB): {}",
                    source.len() / 1024,
                    file_path.display()
                );
                continue;
            }

            let relative_path = file_path
                .strip_prefix(path)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();

            let chunk_offset = entries.len();
            let result =
                treesitter::chunk_file_with_relations(&relative_path, &source, extension);

            for chunk in &result.chunks {
                entries.push(chunk.to_knowledge_entry(&scope_id, &source_version));
            }

            // Offset relation indices by the number of entries collected before this file
            for mut relation in result.relations {
                relation.from_chunk_index += chunk_offset;
                all_relations.push(relation);
            }
        }

        info!(
            "Ingested {} chunks and {} relations from {}",
            entries.len(),
            all_relations.len(),
            source_path
        );
        Ok(IngestionResult {
            entries,
            relations: all_relations,
        })
    }
}

fn get_head_sha(path: &Path) -> Option<String> {
    let repo = Repository::discover(path).ok()?;
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    Some(commit.id().to_string()[..8].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_ingest_nonexistent_path() {
        let adapter = GitAdapter::new();
        let result = adapter.ingest("/nonexistent/path").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ingest_with_relations_rust_files() {
        let dir = TempDir::new().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();

        // Write a Rust file with use statements and an impl
        fs::write(
            src_dir.join("lib.rs"),
            r#"
use std::collections::HashMap;
use crate::foo::Bar;

pub trait MyTrait {
    fn do_thing(&self);
}

pub struct MyStruct {
    field: i32,
}

impl MyTrait for MyStruct {
    fn do_thing(&self) {
        println!("hello");
    }
}
"#,
        )
        .unwrap();

        // Write a second Rust file
        fs::write(
            src_dir.join("foo.rs"),
            r#"
use super::MyTrait;

pub struct Bar {
    val: String,
}
"#,
        )
        .unwrap();

        let adapter = GitAdapter::new();
        let result = adapter
            .ingest_with_relations(dir.path().to_str().unwrap())
            .await
            .unwrap();

        // Should have entries from both files
        assert!(
            result.entries.len() >= 4,
            "Expected at least 4 entries, got {}",
            result.entries.len()
        );

        // Should have import relations
        let imports: Vec<&CodeRelation> = result
            .relations
            .iter()
            .filter(|r| r.relation == "imports")
            .collect();
        assert!(
            imports.len() >= 3,
            "Expected at least 3 import relations, got {}",
            imports.len()
        );

        // Should have an implements relation
        let implements: Vec<&CodeRelation> = result
            .relations
            .iter()
            .filter(|r| r.relation == "implements")
            .collect();
        assert_eq!(
            implements.len(),
            1,
            "Expected 1 implements relation, got {}",
            implements.len()
        );
        assert_eq!(implements[0].to_name.as_deref(), Some("MyTrait"));

        // Verify from_chunk_index offsets are valid (within entries range)
        for rel in &result.relations {
            assert!(
                rel.from_chunk_index < result.entries.len(),
                "from_chunk_index {} out of bounds (entries len: {})",
                rel.from_chunk_index,
                result.entries.len()
            );
        }
    }

    #[tokio::test]
    async fn test_ingest_with_relations_nonexistent_path() {
        let adapter = GitAdapter::new();
        let result = adapter.ingest_with_relations("/nonexistent/path").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ingest_with_relations_empty_dir() {
        let dir = TempDir::new().unwrap();
        let adapter = GitAdapter::new();
        let result = adapter
            .ingest_with_relations(dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert!(result.entries.is_empty());
        assert!(result.relations.is_empty());
    }
}
