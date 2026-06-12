# ContextForge

ContextForge is a local Rust CLI for compiling project files into auditable AI context bundles. The final project will scan local files, extract text, rank relevant chunks, audit privacy risks, and generate bundle, manifest, and report outputs.

## Current Status

Implemented:

- `contextforge init`
- `contextforge scan --source <dir>`
- `contextforge search --source <dir> <query>`
- `contextforge audit --source <dir> [--format text|json]`
- `contextforge pack --source <dir> --goal <text> --budget <n> [--redact] [--fail-on low|medium|high]`
- default `contextforge.toml` generation
- recursive directory scanning with default ignores
- file type and skipped file summaries
- text extraction for Markdown, TXT, Rust, TOML, and JSON
- smart chunking for Markdown sections, Rust items, and plain paragraphs with source line numbers
- explainable deterministic ranking with text, title, path, file-name, file-kind, chunk-kind, and density signals
- budget-aware context selection with per-file budget guardrails and exclusion reasons
- privacy risk auditing for common key, token, database URL, email, phone, private key, URL token, and instruction override patterns
- JSON privacy audit output, severity gates, and optional selected-line redaction during packing
- context bundle, JSON manifest, and Markdown report generation
- typed error handling for config creation
- unit and integration tests

Planned next:

- reporting polish and final demonstration assets

## Build

```powershell
cargo build
```

## Run

```powershell
cargo run -- init
cargo run -- scan --source .
cargo run -- search --source . "ownership borrowing"
cargo run -- audit --source .
cargo run -- audit --source . --format json
cargo run -- pack --source . --goal "ownership borrowing" --budget 500
cargo run -- pack --source . --goal "ownership borrowing" --budget 500 --redact --fail-on high
```

The `init` command writes `contextforge.toml` in the current directory and refuses to overwrite an existing config file.

The `scan` command recursively scans a source directory, skips `.git`, `target`, `node_modules`, oversized files, and binary files, then prints file type and skipped item summaries.

The `search` command scans local text files, extracts supported formats, chunks content by Markdown heading, Rust top-level item, or paragraph, scores chunks against the query, and prints ranked file path, line range, chunk type, optional title, score, preview, and score reason results.

The `audit` command scans local text files for common privacy risk patterns and prints severity, finding type, file path, line number, and a short evidence label. Use `--format json` for machine-readable audit results.

The `pack` command selects relevant chunks for a goal within a token budget, applies a per-file budget guardrail to keep one file from dominating the bundle, runs the privacy audit, and writes `context-bundle.md`, `context-manifest.json`, and `context-report.md` in the current directory. The manifest records chunk type, title, score breakdowns, selection reasons, excluded chunks, budget usage, privacy findings, redaction status, and severity gate behavior. Use `--redact` to replace selected sensitive lines with `[REDACTED: <type>]`, and `--fail-on <severity>` to stop packing when findings meet or exceed the selected severity.

## Test

```powershell
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Project Structure

- `src/main.rs` contains the binary entry point.
- `src/audit/` detects privacy risk patterns in extracted text.
- `src/budget/` selects ranked chunks under global and per-file token budgets.
- `src/cli.rs` parses and dispatches CLI commands.
- `src/chunk/` splits extracted documents into line-aware chunks.
- `src/config.rs` owns default configuration generation.
- `src/error.rs` defines typed project errors.
- `src/extract/` reads supported text formats into documents.
- `src/pack/` generates bundle, manifest, and report outputs.
- `src/rank/` scores chunks and explains ranking decisions.
- `src/scanner/` scans directories and records file metadata.
- `src/search/` ranks chunks against local search queries.
- `tests/cli_init.rs` verifies CLI behavior through the compiled binary.
- `tests/cli_scan.rs` verifies scanner CLI behavior through the compiled binary.
- `tests/cli_search.rs` verifies search CLI behavior through the compiled binary.
- `tests/cli_audit.rs` verifies audit CLI behavior through the compiled binary.
- `tests/cli_pack.rs` verifies pack CLI behavior through the compiled binary.
