<p align="center">
  <img src="docs/assets/corvia-logo.png" alt="corvia" width="200">
</p>

# corvia-adapter-git

[![AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)

Git repository and source code ingestion adapter for [corvia](https://github.com/corvia/corvia), powered by tree-sitter.

## Overview

`corvia-adapter-git` implements the `IngestionAdapter` trait from `corvia-kernel`,
walking a Git repository and parsing source files into structured knowledge entries
using language-specific tree-sitter grammars. It also extracts structural code
relationships (imports, implements, contains) for the kernel's knowledge graph.

## Supported Languages

| Language | Extensions | Constructs |
|----------|-----------|------------|
| Rust | `.rs` | functions, structs, enums, impls, traits, modules |
| JavaScript | `.js`, `.jsx` | functions, classes, arrow functions, exports |
| TypeScript | `.ts`, `.tsx` | functions, classes, interfaces, type aliases |
| Python | `.py` | functions, classes, decorators, imports |
| Markdown | `.md` | headings, code blocks, sections |
| Config | `.toml`, `.yaml`, `.json` | key-value structures |

## Usage

```rust,no_run
use corvia_adapter_git::GitAdapter;
use corvia_kernel::traits::IngestionAdapter;

async fn example() -> corvia_common::errors::Result<()> {
    let adapter = GitAdapter::new();
    let entries = adapter.ingest("/path/to/repo").await?;
    // entries are KnowledgeEntry values without embeddings —
    // the kernel adds embeddings after ingestion.
    Ok(())
}
```

## Exports

- **`GitAdapter`** — Main adapter struct implementing `IngestionAdapter`
- **`IngestionResult`** — Summary of ingested entries and extracted relations
- **`CodeRelation`** — Structural relationship between code entities
- **`ChunkResult`** — Individual parsed chunk from a source file

## Related

- [corvia](https://github.com/corvia/corvia) — The main knowledge system
- [corvia-kernel](https://github.com/corvia/corvia/tree/master/crates/corvia-kernel) — Core traits and storage
- [tree-sitter](https://tree-sitter.github.io/) — Parser framework

## License

AGPL-3.0-only — see [LICENSE](LICENSE) for details.
