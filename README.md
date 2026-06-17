# ContextForge

ContextForge is a local Rust CLI for compiling project files into auditable context bundles. The final project will scan local files, extract text, rank relevant chunks, audit privacy risks, and generate bundle, manifest, and report outputs.

## Current Status

Implemented:

- `contextforge init`
- `contextforge scan --source <dir>`
- `contextforge search --source <dir> <query>`
- `contextforge audit --source <dir> [--format text|json]`
- `contextforge metrics --source <dir> [--format text|json]`
- `contextforge pack --source <dir> --goal <text> --budget <n> [--output-dir <dir>] [--redact] [--fail-on low|medium|high]`
- default `contextforge.toml` generation
- automatic `contextforge.toml` loading, with optional global `--config <path>`
- recursive directory scanning with default ignores
- file type and skipped file summaries
- text extraction for Markdown, TXT/log/config text, Rust, common code files, TOML, JSON, YAML, CSV, TSV, XML, HTML, PDF, and DOCX
- smart chunking for Markdown sections, Rust items, common code items, table rows, and plain paragraphs with source line numbers
- explainable deterministic ranking with text, title, path, file-name, file-kind, chunk-kind, and density signals
- budget-aware context selection with per-file budget guardrails and exclusion reasons
- privacy risk auditing for common key, token, database URL, email, phone, private key, URL token, and instruction override patterns
- JSON privacy audit output, severity gates, and optional selected-line redaction during packing
- Rust project metrics for effective lines, module/type/test signals, risk calls, and requirement checks
- configurable output directory and output file names
- context bundle, JSON manifest, and Markdown report generation with selection and privacy statistics
- typed error handling for config creation
- unit and integration tests

Planned next:

- ranking algorithm improvements and final demonstration assets

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
cargo run -- metrics --source .
cargo run -- metrics --source . --format json
cargo run -- pack --source . --goal "ownership borrowing" --budget 500
cargo run -- pack --source . --goal "ownership borrowing" --budget 500 --output-dir out
cargo run -- pack --source . --goal "ownership borrowing" --budget 500 --redact --fail-on high
cargo run -- --config .\contextforge.toml scan --source .
```

The `init` command writes `contextforge.toml` in the current directory and refuses to overwrite an existing config file. Commands automatically load `contextforge.toml` from the current directory when present. Use the global `--config <path>` option to load a specific configuration file.

Currently supported configuration fields:

```toml
[scanner]
max_file_bytes = 1048576
ignore_patterns = [".git", "target", "node_modules"]

[output]
bundle = "context-bundle.md"
manifest = "context-manifest.json"
report = "context-report.md"
```

The `scan` command recursively scans a source directory, skips configured ignored directories, oversized files, and unsupported binary files, then prints file type and skipped item summaries. Supported binary document formats such as PDF and DOCX are kept for extraction.

The `search` command scans local files, extracts supported formats, chunks content by Markdown heading, Rust top-level item, common code item, table row group, or paragraph, scores chunks against the query, and prints ranked file path, line range, chunk type, optional title, score, preview, and score reason results. PDF and DOCX files are extracted into plain text before chunking, and HTML/XML files are reduced to readable text before ranking.

Supported plain-text and structured formats include:

- Documents and notes: `.md`, `.markdown`, `.txt`, `.text`, `.log`, `.out`, `.err`
- Config/data: `.toml`, `.json`, `.yaml`, `.yml`, `.csv`, `.tsv`, `.xml`, `.xsd`, `.svg`, `.html`, `.htm`, `.ini`, `.cfg`, `.conf`, `.properties`, `.env*`
- Code: `.rs`, `.py`, `.js`, `.jsx`, `.ts`, `.tsx`, `.java`, `.c`, `.h`, `.cc`, `.cpp`, `.cxx`, `.hpp`, `.cs`, `.go`, `.rb`, `.php`, `.swift`, `.kt`, `.kts`, `.scala`, `.sh`, `.bash`, `.zsh`, `.ps1`, `.sql`, `.lua`, `.r`, `.m`, `.mm`, `.dart`, `.ex`, `.exs`, `.clj`, `.cljs`, `.fs`, `.fsx`, `.vb`, `.gradle`, plus common files such as `Dockerfile`, `Makefile`, `Justfile`, `Gemfile`, and `Jenkinsfile`
- Binary document extraction: `.pdf`, `.docx`

The `audit` command scans extracted text for common privacy risk patterns and prints severity, finding type, file path, line number, and a short evidence label. Use `--format json` for machine-readable audit results.

The `metrics` command analyzes Rust source files while skipping generated/build directories. It reports total and effective Rust lines, `src` and `tests` line counts, module declarations, `struct`/`enum`/`trait`/`impl` usage, function and test counts, `Result` usage, and risk signals such as `unwrap`, `expect`, `panic!`, `todo!`, and `unsafe`. Its requirement signals are intended to help judge whether the project visibly satisfies the course's engineering expectations.

The `pack` command selects relevant chunks for a goal within a token budget, applies a per-file budget guardrail to keep one file from dominating the bundle, runs the privacy audit, and writes `context-bundle.md`, `context-manifest.json`, and `context-report.md` in the current directory or a directory supplied with `--output-dir`. The manifest records chunk type, title, score breakdowns, selection reasons, excluded chunks, budget usage, privacy findings, redaction status, selected chunk type counts, privacy severity counts, and privacy finding type counts. Use `--redact` to replace selected sensitive lines with `[REDACTED: <type>]`, and `--fail-on <severity>` to stop packing when findings meet or exceed the selected severity.

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
- `src/metrics/` analyzes Rust project scale, feature signals, tests, and risk calls.
- `src/pack/` generates bundle, manifest, and report outputs.
- `src/rank/` scores chunks and explains ranking decisions.
- `src/scanner/` scans directories and records file metadata.
- `src/search/` ranks chunks against local search queries.
- `tests/cli_init.rs` verifies CLI behavior through the compiled binary.
- `tests/cli_scan.rs` verifies scanner CLI behavior through the compiled binary.
- `tests/cli_search.rs` verifies search CLI behavior through the compiled binary.
- `tests/cli_audit.rs` verifies audit CLI behavior through the compiled binary.
- `tests/cli_metrics.rs` verifies metrics CLI behavior through the compiled binary.
- `tests/cli_pack.rs` verifies pack CLI behavior through the compiled binary.
