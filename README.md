# ContextForge

ContextForge is a local Rust CLI for compiling project files into auditable AI context bundles. The final project will scan local files, extract text, rank relevant chunks, audit privacy risks, and generate bundle, manifest, and report outputs.

## Current Status

Implemented:

- `contextforge init`
- `contextforge scan --source <dir>`
- default `contextforge.toml` generation
- recursive directory scanning with default ignores
- file type and skipped file summaries
- typed error handling for config creation
- unit and integration tests

Planned next:

- text extraction for Markdown, TXT, Rust, TOML, and JSON
- `search`, `audit`, and `pack`

## Build

```powershell
cargo build
```

## Run

```powershell
cargo run -- init
cargo run -- scan --source .
```

The `init` command writes `contextforge.toml` in the current directory and refuses to overwrite an existing config file.

The `scan` command recursively scans a source directory, skips `.git`, `target`, `node_modules`, oversized files, and binary files, then prints file type and skipped item summaries.

## Test

```powershell
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Project Structure

- `src/main.rs` contains the binary entry point.
- `src/cli.rs` parses and dispatches CLI commands.
- `src/config.rs` owns default configuration generation.
- `src/error.rs` defines typed project errors.
- `src/scanner/` scans directories and records file metadata.
- `tests/cli_init.rs` verifies CLI behavior through the compiled binary.
- `tests/cli_scan.rs` verifies scanner CLI behavior through the compiled binary.
