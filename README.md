# ContextForge

ContextForge is a local Rust CLI for compiling project files into auditable AI context bundles. The final project will scan local files, extract text, rank relevant chunks, audit privacy risks, and generate bundle, manifest, and report outputs.

## Current Status

Implemented:

- `contextforge init`
- `contextforge scan --source <dir>`
- `contextforge search --source <dir> <query>`
- default `contextforge.toml` generation
- recursive directory scanning with default ignores
- file type and skipped file summaries
- text extraction for Markdown, TXT, Rust, TOML, and JSON
- paragraph chunking with source line numbers
- deterministic keyword-based local search
- typed error handling for config creation
- unit and integration tests

Planned next:

- `audit`
- `pack`

## Build

```powershell
cargo build
```

## Run

```powershell
cargo run -- init
cargo run -- scan --source .
cargo run -- search --source . "ownership borrowing"
```

The `init` command writes `contextforge.toml` in the current directory and refuses to overwrite an existing config file.

The `scan` command recursively scans a source directory, skips `.git`, `target`, `node_modules`, oversized files, and binary files, then prints file type and skipped item summaries.

The `search` command scans local text files, extracts supported formats, chunks content by paragraph, scores chunks against the query, and prints ranked file path, line number, score, and preview results.

## Test

```powershell
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Project Structure

- `src/main.rs` contains the binary entry point.
- `src/cli.rs` parses and dispatches CLI commands.
- `src/chunk/` splits extracted documents into line-aware chunks.
- `src/config.rs` owns default configuration generation.
- `src/error.rs` defines typed project errors.
- `src/extract/` reads supported text formats into documents.
- `src/scanner/` scans directories and records file metadata.
- `src/search/` ranks chunks against local search queries.
- `tests/cli_init.rs` verifies CLI behavior through the compiled binary.
- `tests/cli_scan.rs` verifies scanner CLI behavior through the compiled binary.
- `tests/cli_search.rs` verifies search CLI behavior through the compiled binary.
