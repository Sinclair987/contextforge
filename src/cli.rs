use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use crate::{
    audit::{PrivacyFinding, Severity},
    config::{load_config, write_default_config, AppConfig, DEFAULT_CONFIG_FILE},
    metrics::{analyze_rust_project, RustFileMetrics, RustProjectMetrics},
    pack::{pack_directory_with_options, PackFileNames, PackOptions, PackResult},
    scanner::{scan_directory, FileKind, ScanSummary, SkipReason},
    search::{search_directory_with_options, SearchHit},
    ContextForgeError, Result,
};

#[derive(Debug, Parser)]
#[command(name = "contextforge")]
#[command(about = "Compile local project files into auditable context bundles")]
pub struct Cli {
    /// Optional configuration file. Defaults to contextforge.toml in the current directory when present.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate a sample contextforge.toml configuration file.
    Init,

    /// Recursively scan a source directory and summarize readable files.
    Scan {
        /// Directory to scan.
        #[arg(long)]
        source: PathBuf,
    },

    /// Search supported local files for relevant context.
    Search {
        /// Directory to search.
        #[arg(long)]
        source: PathBuf,

        /// Search query.
        query: String,
    },

    /// Audit supported local files for privacy risks.
    Audit {
        /// Directory to audit.
        #[arg(long)]
        source: PathBuf,

        /// Output format.
        #[arg(long, value_enum, default_value_t = AuditFormat::Text)]
        format: AuditFormat,
    },

    /// Analyze Rust source metrics and course requirement signals.
    Metrics {
        /// Directory to analyze.
        #[arg(long)]
        source: PathBuf,

        /// Output format.
        #[arg(long, value_enum, default_value_t = MetricsFormat::Text)]
        format: MetricsFormat,
    },

    /// Generate a context bundle, manifest, and report.
    Pack {
        /// Directory to pack.
        #[arg(long)]
        source: PathBuf,

        /// Context goal used to rank source chunks.
        #[arg(long)]
        goal: String,

        /// Maximum estimated tokens for selected context.
        #[arg(long)]
        budget: usize,

        /// Directory where bundle, manifest, and report are written.
        #[arg(long, default_value = ".")]
        output_dir: PathBuf,

        /// Replace selected sensitive lines with redaction markers.
        #[arg(long)]
        redact: bool,

        /// Preview selection and privacy checks without writing output files.
        #[arg(long)]
        dry_run: bool,

        /// Fail if the privacy audit finds this severity or higher.
        #[arg(long, value_enum)]
        fail_on: Option<CliSeverity>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AuditFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MetricsFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliSeverity {
    Low,
    Medium,
    High,
}

impl From<CliSeverity> for Severity {
    fn from(value: CliSeverity) -> Self {
        match value {
            CliSeverity::Low => Self::Low,
            CliSeverity::Medium => Self::Medium,
            CliSeverity::High => Self::High,
        }
    }
}

#[derive(Debug, Serialize)]
struct AuditJsonReport {
    findings: Vec<AuditJsonFinding>,
}

#[derive(Debug, Serialize)]
struct AuditJsonFinding {
    path: String,
    line: usize,
    kind: String,
    severity: String,
    evidence: String,
}

pub fn run<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::Init => init_current_directory(),
        Commands::Scan { source } => {
            let config = load_config(cli.config.as_deref())?;
            scan_source_directory(&source, &config)
        }
        Commands::Search { source, query } => {
            let config = load_config(cli.config.as_deref())?;
            search_source_directory(&source, &query, &config)
        }
        Commands::Audit { source, format } => {
            let config = load_config(cli.config.as_deref())?;
            audit_source_directory(&source, format, &config)
        }
        Commands::Metrics { source, format } => metrics_source_directory(&source, format),
        Commands::Pack {
            source,
            goal,
            budget,
            output_dir,
            redact,
            dry_run,
            fail_on,
        } => {
            let config = load_config(cli.config.as_deref())?;
            pack_source_directory(
                PackCommandOptions {
                    source: &source,
                    goal: &goal,
                    budget,
                    output_dir: &output_dir,
                    redact,
                    dry_run,
                    fail_on: fail_on.map(Severity::from),
                },
                &config,
            )
        }
    }
}

fn init_current_directory() -> Result<()> {
    let path = PathBuf::from(DEFAULT_CONFIG_FILE);
    write_default_config(&path)?;
    println!("Created {}", path.display());
    Ok(())
}

fn scan_source_directory(source: &Path, config: &AppConfig) -> Result<()> {
    let summary = scan_directory(source, &config.scan_options())?;
    print_scan_summary(&summary);
    Ok(())
}

fn print_scan_summary(summary: &ScanSummary) {
    println!("Scanned files: {}", summary.files.len());
    println!("Skipped files: {}", summary.skipped.len());
    println!();
    println!("File types:");
    print_kind_count(summary, FileKind::Markdown);
    print_kind_count(summary, FileKind::Rust);
    print_kind_count(summary, FileKind::Code);
    print_kind_count(summary, FileKind::Text);
    print_kind_count(summary, FileKind::Toml);
    print_kind_count(summary, FileKind::Json);
    print_kind_count(summary, FileKind::Yaml);
    print_kind_count(summary, FileKind::Csv);
    print_kind_count(summary, FileKind::Tsv);
    print_kind_count(summary, FileKind::Xml);
    print_kind_count(summary, FileKind::Html);
    print_kind_count(summary, FileKind::Pdf);
    print_kind_count(summary, FileKind::Docx);
    print_kind_count(summary, FileKind::Other);
    println!();
    println!("Skipped:");
    print_skip_count(summary, SkipReason::IgnoredDirectory);
    print_skip_count(summary, SkipReason::TooLarge);
    print_skip_count(summary, SkipReason::Binary);
}

fn print_kind_count(summary: &ScanSummary, kind: FileKind) {
    let count = summary.count_by_kind(kind);
    if count > 0 {
        println!("  {}: {count}", kind.label());
    }
}

fn print_skip_count(summary: &ScanSummary, reason: SkipReason) {
    let count = summary.count_by_skip_reason(reason);
    if count > 0 {
        println!("  {}: {count}", reason.label());
    }
}

fn search_source_directory(source: &Path, query: &str, config: &AppConfig) -> Result<()> {
    let hits = search_directory_with_options(source, query, &config.scan_options())?;
    print_search_hits(query, &hits);
    Ok(())
}

fn print_search_hits(query: &str, hits: &[SearchHit]) {
    println!("Search results for: {query}");

    if hits.is_empty() {
        println!("No matches found.");
        return;
    }

    for (index, hit) in hits.iter().enumerate() {
        let rank = index + 1;
        println!(
            "{rank}. {}: lines {}-{} | {} | score {}",
            hit.path.display(),
            hit.start_line,
            hit.end_line,
            hit.kind.label(),
            hit.score
        );
        if let Some(title) = &hit.title {
            println!("   title: {title}");
        }
        println!("   {}", hit.preview);
        println!("   reason: {}", hit.score_breakdown.summary());
    }
}

fn audit_source_directory(source: &Path, format: AuditFormat, config: &AppConfig) -> Result<()> {
    let findings = crate::audit::audit_directory_with_options(source, &config.scan_options())?;
    match format {
        AuditFormat::Text => print_audit_findings(&findings),
        AuditFormat::Json => print_json_audit_findings(&findings)?,
    }
    Ok(())
}

fn print_audit_findings(findings: &[PrivacyFinding]) {
    println!("Privacy findings: {}", findings.len());

    if findings.is_empty() {
        println!("No privacy risks found.");
        return;
    }

    for finding in findings {
        println!(
            "{} | {} | {}: line {} | {}",
            finding.severity.label(),
            finding.kind.label(),
            finding.path.display(),
            finding.line,
            finding.evidence
        );
    }
}

fn print_json_audit_findings(findings: &[PrivacyFinding]) -> Result<()> {
    let report = AuditJsonReport {
        findings: findings
            .iter()
            .map(|finding| AuditJsonFinding {
                path: finding.path.display().to_string(),
                line: finding.line,
                kind: finding.kind.label().to_string(),
                severity: finding.severity.label().to_string(),
                evidence: finding.evidence.clone(),
            })
            .collect(),
    };
    let output = serde_json::to_string_pretty(&report)
        .map_err(|source| ContextForgeError::SerializeOutput { source })?;
    println!("{output}");
    Ok(())
}

fn metrics_source_directory(source: &Path, format: MetricsFormat) -> Result<()> {
    let metrics = analyze_rust_project(source)?;
    match format {
        MetricsFormat::Text => print_metrics(&metrics),
        MetricsFormat::Json => print_json_metrics(&metrics)?,
    }
    Ok(())
}

fn print_metrics(metrics: &RustProjectMetrics) {
    let summary = &metrics.summary;
    println!("Rust project metrics");
    println!("Source: {}", metrics.source.display());
    println!("Rust files: {}", summary.rust_files);
    println!("Source files: {}", summary.source_files);
    println!("Test files: {}", summary.test_files);
    println!("Total lines: {}", summary.total_lines);
    println!("Effective lines: {}", summary.effective_lines);
    println!("Effective lines in src: {}", summary.source_effective_lines);
    println!("Effective lines in tests: {}", summary.test_effective_lines);
    println!("Blank lines: {}", summary.blank_lines);
    println!("Comment-only lines: {}", summary.comment_lines);
    println!();
    println!("Rust feature signals:");
    println!("  Modules declared: {}", summary.modules_declared);
    println!("  Structs: {}", summary.structs);
    println!("  Enums: {}", summary.enums);
    println!("  Traits: {}", summary.traits);
    println!("  Impl blocks: {}", summary.impl_blocks);
    println!("  Functions: {}", summary.functions);
    println!("  Public item lines: {}", summary.public_items);
    println!("  Async functions: {}", summary.async_functions);
    println!("  Generic item lines: {}", summary.generic_item_lines);
    println!("  Lifetime lines: {}", summary.lifetime_lines);
    println!("  Result mentions: {}", summary.result_mentions);
    println!("  Test functions: {}", summary.test_functions);
    println!();
    println!("Risk signals:");
    println!("  unwrap calls: {}", summary.unwrap_calls);
    println!("  expect calls: {}", summary.expect_calls);
    println!("  panic macros: {}", summary.panic_macros);
    println!("  todo/unimplemented macros: {}", summary.todo_macros);
    println!("  unsafe mentions: {}", summary.unsafe_mentions);
    println!();
    println!("Requirement signals:");
    for check in &metrics.assessment.checks {
        println!(
            "  [{}] {} - {}",
            check.status.label(),
            check.name,
            check.detail
        );
    }
    println!();
    println!("Largest Rust files:");
    for file in largest_files(&metrics.files, 8) {
        println!(
            "  {} | {} | {} effective / {} total lines | {} fn | {} tests",
            file.path.display(),
            file.area.label(),
            file.effective_lines,
            file.total_lines,
            file.functions,
            file.test_functions
        );
    }
}

fn largest_files(files: &[RustFileMetrics], limit: usize) -> Vec<&RustFileMetrics> {
    let mut sorted = files.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| {
        right
            .effective_lines
            .cmp(&left.effective_lines)
            .then_with(|| left.path.cmp(&right.path))
    });
    sorted.into_iter().take(limit).collect()
}

fn print_json_metrics(metrics: &RustProjectMetrics) -> Result<()> {
    let output = serde_json::to_string_pretty(metrics)
        .map_err(|source| ContextForgeError::SerializeOutput { source })?;
    println!("{output}");
    Ok(())
}

struct PackCommandOptions<'a> {
    source: &'a Path,
    goal: &'a str,
    budget: usize,
    output_dir: &'a Path,
    redact: bool,
    dry_run: bool,
    fail_on: Option<Severity>,
}

fn pack_source_directory(options: PackCommandOptions<'_>, config: &AppConfig) -> Result<()> {
    let output = config.output_values();
    let result = pack_directory_with_options(
        options.source,
        options.goal,
        options.budget,
        options.output_dir,
        PackOptions {
            redact: options.redact,
            fail_on: options.fail_on,
            scan_options: config.scan_options(),
            file_names: PackFileNames::from(output),
            write_outputs: !options.dry_run,
        },
    )?;
    if options.dry_run {
        print_pack_dry_run_result(&result);
    } else {
        print_pack_result(&result);
    }
    Ok(())
}

fn print_pack_result(result: &PackResult) {
    println!("Generated {}", display_output_path(&result.bundle_path));
    println!("Generated {}", display_output_path(&result.manifest_path));
    println!("Generated {}", display_output_path(&result.report_path));
    println!("Selected chunks: {}", result.selected_chunks.len());
    println!("Excluded chunks: {}", result.excluded_chunks.len());
    println!("Used tokens: {}", result.used_tokens);
    println!("Remaining tokens: {}", result.remaining_tokens);
    println!("Privacy findings: {}", result.privacy_findings.len());
    println!(
        "Redaction: {}",
        if result.redaction_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
}

fn print_pack_dry_run_result(result: &PackResult) {
    println!("Dry run: no files written");
    println!("Would write {}", display_output_path(&result.bundle_path));
    println!("Would write {}", display_output_path(&result.manifest_path));
    println!("Would write {}", display_output_path(&result.report_path));
    println!("Selected chunks: {}", result.selected_chunks.len());
    println!("Excluded chunks: {}", result.excluded_chunks.len());
    println!("Used tokens: {}", result.used_tokens);
    println!("Remaining tokens: {}", result.remaining_tokens);
    println!(
        "per-file budget limit: {}",
        result.budget_policy.per_file_budget_limit()
    );
    println!("Privacy findings: {}", result.privacy_findings.len());
    println!();
    println!("Selected preview:");
    if result.selected_chunks.is_empty() {
        println!("  none");
    } else {
        for chunk in result.selected_chunks.iter().take(5) {
            println!(
                "  {}: lines {}-{} | {} | score {} | tokens {}",
                chunk.path.display(),
                chunk.start_line,
                chunk.end_line,
                chunk.kind.label(),
                chunk.score,
                chunk.token_estimate
            );
            println!("    {}", chunk.preview);
            println!("    reason: {}", chunk.selection_reason);
        }
    }
    println!();
    println!("Excluded preview:");
    if result.excluded_chunks.is_empty() {
        println!("  none");
    } else {
        for chunk in result.excluded_chunks.iter().take(5) {
            println!(
                "  {}: lines {}-{} | score {} | tokens {}",
                chunk.path.display(),
                chunk.start_line,
                chunk.end_line,
                chunk.score,
                chunk.token_estimate
            );
            println!("    {}", chunk.preview);
            println!("    reason: {}", chunk.reason);
        }
    }
}

fn display_output_path(path: &Path) -> String {
    path.strip_prefix(".").unwrap_or(path).display().to_string()
}
