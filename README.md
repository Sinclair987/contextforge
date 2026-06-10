# ContextForge

ContextForge is a local Rust CLI for compiling project files into auditable AI context bundles. The final project will scan local files, extract text, rank relevant chunks, audit privacy risks, and generate bundle, manifest, and report outputs.

## Phase 1 Status

Implemented:

- `contextforge init`
- default `contextforge.toml` generation
- typed error handling for config creation
- unit and integration tests for the first CLI slice

Planned next:

- `scan --source <dir>`
- text extraction for Markdown, TXT, Rust, TOML, and JSON
- `search`, `audit`, and `pack`

## Build

```powershell
cargo build
```

## Run

```powershell
cargo run -- init
```

The command writes `contextforge.toml` in the current directory and refuses to overwrite an existing config file.

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
- `tests/cli_init.rs` verifies CLI behavior through the compiled binary.
