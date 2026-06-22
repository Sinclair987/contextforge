use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use crate::{
    audit::{audit_directory_report_with_options, PrivacyFinding, Severity},
    config::{load_config_for_source, write_default_config, AppConfig, DEFAULT_CONFIG_FILE},
    corpus::ExtractionIssue,
    pack::{pack_directory_with_options, PackFileNames, PackOptions, PackResult},
    scanner::{scan_directory, FileKind, ScanSummary, SkipReason},
    search::{search_directory_report_with_options, SearchHit},
    ContextForgeError, Result,
};

#[derive(Debug, Parser)]
#[command(name = "contextforge")]
#[command(about = "Compile local project files into auditable context bundles")]
#[command(version)]
pub struct Cli {
    /// Optional configuration file. Defaults to the source directory, then the current directory.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(name = "__extract-pdf-worker", hide = true)]
    ExtractPdfWorker { input: PathBuf, output: PathBuf },

    /// Generate a sample contextforge.toml configuration file.
    Init {
        /// Directory where contextforge.toml is created.
        #[arg(short, long, default_value = ".")]
        source: PathBuf,
    },

    /// Recursively scan a source directory and summarize readable files.
    Scan {
        /// Directory to scan.
        #[arg(short, long, default_value = ".")]
        source: PathBuf,

        /// Only scan these relative paths. May be repeated.
        #[arg(long, value_name = "PATH")]
        include: Vec<PathBuf>,

        /// Exclude these relative paths. May be repeated.
        #[arg(long, value_name = "PATH")]
        exclude: Vec<PathBuf>,
    },

    /// Search supported local files for relevant context.
    Search {
        /// Directory to search.
        #[arg(short, long, default_value = ".")]
        source: PathBuf,

        /// Search query.
        query: String,

        /// Maximum results to print. Use 0 for all results.
        #[arg(short, long, default_value_t = 10)]
        limit: usize,

        /// Only search these relative paths. May be repeated.
        #[arg(long, value_name = "PATH")]
        include: Vec<PathBuf>,

        /// Exclude these relative paths. May be repeated.
        #[arg(long, value_name = "PATH")]
        exclude: Vec<PathBuf>,
    },

    /// Audit supported local files for privacy risks.
    Audit {
        /// Directory to audit.
        #[arg(short, long, default_value = ".")]
        source: PathBuf,

        /// Output format.
        #[arg(long, value_enum, default_value_t = AuditFormat::Text)]
        format: AuditFormat,

        /// Maximum findings to print in text format. Use 0 for all findings.
        #[arg(short, long, default_value_t = 50)]
        limit: usize,

        /// Only audit these relative paths. May be repeated.
        #[arg(long, value_name = "PATH")]
        include: Vec<PathBuf>,

        /// Exclude these relative paths. May be repeated.
        #[arg(long, value_name = "PATH")]
        exclude: Vec<PathBuf>,
    },

    /// Generate a context bundle, manifest, and report.
    Pack {
        /// Directory to pack.
        #[arg(short, long, default_value = ".")]
        source: PathBuf,

        /// Context goal used to rank source chunks.
        #[arg(value_name = "GOAL", required_unless_present = "goal_option")]
        goal: Option<String>,

        /// Context goal used to rank source chunks.
        #[arg(long = "goal", value_name = "GOAL", conflicts_with = "goal")]
        goal_option: Option<String>,

        /// Maximum estimated tokens for selected context.
        #[arg(short, long, default_value_t = 6000)]
        budget: usize,

        /// Directory where bundle, manifest, and report are written.
        #[arg(short, long)]
        output_dir: Option<PathBuf>,

        /// Replace selected sensitive lines with redaction markers.
        #[arg(long)]
        redact: bool,

        /// Preview selection and privacy checks without writing output files.
        #[arg(long)]
        dry_run: bool,

        /// Fail if the privacy audit finds this severity or higher.
        #[arg(long, value_enum)]
        fail_on: Option<CliSeverity>,

        /// Only pack these relative paths. May be repeated.
        #[arg(long, value_name = "PATH")]
        include: Vec<PathBuf>,

        /// Exclude these relative paths. May be repeated.
        #[arg(long, value_name = "PATH")]
        exclude: Vec<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AuditFormat {
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
    extraction_issues: Vec<AuditJsonExtractionIssue>,
}

#[derive(Debug, Serialize)]
struct AuditJsonFinding {
    path: String,
    line: usize,
    kind: String,
    severity: String,
    evidence: String,
}

#[derive(Debug, Serialize)]
struct AuditJsonExtractionIssue {
    path: String,
    message: String,
}

pub fn run<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::ExtractPdfWorker { input, output } => {
            crate::extract::write_pdf_worker_output(&input, &output)
        }
        Commands::Init { source } => init_source_directory(&source),
        Commands::Scan {
            source,
            include,
            exclude,
        } => {
            let config = load_config_for_source(cli.config.as_deref(), &source)?;
            let scan_options = scan_options_with_filters(&config, include, exclude);
            scan_source_directory(&source, &scan_options)
        }
        Commands::Search {
            source,
            query,
            limit,
            include,
            exclude,
        } => {
            let config = load_config_for_source(cli.config.as_deref(), &source)?;
            let scan_options = scan_options_with_filters(&config, include, exclude);
            search_source_directory(&source, &query, limit, &scan_options)
        }
        Commands::Audit {
            source,
            format,
            limit,
            include,
            exclude,
        } => {
            let config = load_config_for_source(cli.config.as_deref(), &source)?;
            let scan_options = scan_options_with_filters(&config, include, exclude);
            audit_source_directory(&source, format, limit, &scan_options)
        }
        Commands::Pack {
            source,
            goal,
            goal_option,
            budget,
            output_dir,
            redact,
            dry_run,
            fail_on,
            include,
            exclude,
        } => {
            let config = load_config_for_source(cli.config.as_deref(), &source)?;
            let goal = goal.or(goal_option).unwrap_or_default();
            let output_dir = output_dir.unwrap_or_else(|| source.join("contextforge-output"));
            pack_source_directory(
                PackCommandOptions {
                    source: &source,
                    goal: &goal,
                    budget,
                    output_dir: &output_dir,
                    redact,
                    dry_run,
                    fail_on: fail_on.map(Severity::from),
                    include,
                    exclude,
                },
                &config,
            )
        }
    }
}

fn init_source_directory(source: &Path) -> Result<()> {
    let path = source.join(DEFAULT_CONFIG_FILE);
    write_default_config(&path)?;
    println!(
        "Created {}",
        crate::paths::relative_display(Path::new("."), &path)
    );
    Ok(())
}

fn scan_options_with_filters(
    config: &AppConfig,
    include: Vec<PathBuf>,
    exclude: Vec<PathBuf>,
) -> crate::scanner::ScanOptions {
    let mut options = config.scan_options();
    options.included_paths.extend(include);
    options.excluded_paths.extend(exclude);
    options
}

fn scan_source_directory(source: &Path, scan_options: &crate::scanner::ScanOptions) -> Result<()> {
    let summary = scan_directory(source, scan_options)?;
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
    print_kind_count(summary, FileKind::Epub);
    print_kind_count(summary, FileKind::Other);
    println!();
    println!("Skipped:");
    print_skip_count(summary, SkipReason::IgnoredDirectory);
    print_skip_count(summary, SkipReason::FilteredPath);
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

fn search_source_directory(
    source: &Path,
    query: &str,
    limit: usize,
    scan_options: &crate::scanner::ScanOptions,
) -> Result<()> {
    let result = search_directory_report_with_options(source, query, scan_options)?;
    print_search_hits(source, query, &result.hits, limit);
    print_extraction_issues(source, &result.extraction_issues);
    Ok(())
}

fn print_search_hits(source: &Path, query: &str, hits: &[SearchHit], limit: usize) {
    println!("Search results for: {query}");

    if hits.is_empty() {
        println!("No matches found.");
        return;
    }

    let shown = if limit == 0 {
        hits.len()
    } else {
        hits.len().min(limit)
    };
    for (index, hit) in hits.iter().take(shown).enumerate() {
        let rank = index + 1;
        println!(
            "{rank}. {}: lines {}-{} | {} | score {}",
            crate::paths::relative_display(source, &hit.path),
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

    if shown < hits.len() {
        println!("Showing {shown} of {} matches.", hits.len());
    }
}

fn audit_source_directory(
    source: &Path,
    format: AuditFormat,
    limit: usize,
    scan_options: &crate::scanner::ScanOptions,
) -> Result<()> {
    let result = audit_directory_report_with_options(source, scan_options)?;
    match format {
        AuditFormat::Text => {
            print_audit_findings(source, &result.findings, limit);
            print_extraction_issues(source, &result.extraction_issues);
        }
        AuditFormat::Json => {
            print_json_audit_findings(source, &result.findings, &result.extraction_issues)?;
            print_extraction_issue_details(source, &result.extraction_issues);
        }
    }
    Ok(())
}

fn print_audit_findings(source: &Path, findings: &[PrivacyFinding], limit: usize) {
    println!("Privacy findings: {}", findings.len());

    if findings.is_empty() {
        println!("No privacy risks found.");
        return;
    }

    let shown = if limit == 0 {
        findings.len()
    } else {
        findings.len().min(limit)
    };
    for finding in findings.iter().take(shown) {
        println!(
            "{} | {} | {}: line {} | {}",
            finding.severity.label(),
            finding.kind.label(),
            crate::paths::relative_display(source, &finding.path),
            finding.line,
            finding.evidence
        );
    }
    if shown < findings.len() {
        println!("Showing {shown} of {} findings.", findings.len());
    }
}

fn print_json_audit_findings(
    source: &Path,
    findings: &[PrivacyFinding],
    extraction_issues: &[ExtractionIssue],
) -> Result<()> {
    let report = AuditJsonReport {
        findings: findings
            .iter()
            .map(|finding| AuditJsonFinding {
                path: crate::paths::relative_display(source, &finding.path),
                line: finding.line,
                kind: finding.kind.label().to_string(),
                severity: finding.severity.label().to_string(),
                evidence: finding.evidence.clone(),
            })
            .collect(),
        extraction_issues: extraction_issues
            .iter()
            .map(|issue| AuditJsonExtractionIssue {
                path: crate::paths::relative_display(source, &issue.path),
                message: issue.message.clone(),
            })
            .collect(),
    };
    let output = serde_json::to_string_pretty(&report)
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
    include: Vec<PathBuf>,
    exclude: Vec<PathBuf>,
}

fn pack_source_directory(options: PackCommandOptions<'_>, config: &AppConfig) -> Result<()> {
    let output = config.output_values();
    let scan_options = scan_options_with_filters(config, options.include, options.exclude);
    let result = pack_directory_with_options(
        options.source,
        options.goal,
        options.budget,
        options.output_dir,
        PackOptions {
            redact: options.redact,
            fail_on: options.fail_on,
            scan_options,
            file_names: PackFileNames::from(output),
            write_outputs: !options.dry_run,
        },
    )?;
    if options.dry_run {
        print_pack_dry_run_result(options.source, &result);
    } else {
        print_pack_result(options.source, &result);
    }
    Ok(())
}

fn print_pack_result(source: &Path, result: &PackResult) {
    println!("Generated {}", display_output_path(&result.bundle_path));
    println!("Generated {}", display_output_path(&result.manifest_path));
    println!("Generated {}", display_output_path(&result.report_path));
    println!("Selected chunks: {}", result.selected_chunks.len());
    println!("Excluded chunks: {}", result.excluded_chunks.len());
    println!("Used tokens: {}", result.used_tokens);
    println!("Remaining tokens: {}", result.remaining_tokens);
    println!(
        "Selected privacy findings: {}",
        result.selected_privacy_findings.len()
    );
    println!("Source privacy findings: {}", result.privacy_findings.len());
    print_extraction_issues(source, &result.extraction_issues);
    println!(
        "Redaction: {}",
        if result.redaction_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
}

fn print_pack_dry_run_result(source: &Path, result: &PackResult) {
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
    println!(
        "Selected privacy findings: {}",
        result.selected_privacy_findings.len()
    );
    println!("Source privacy findings: {}", result.privacy_findings.len());
    print_extraction_issues(source, &result.extraction_issues);
    println!();
    println!("Selected preview:");
    if result.selected_chunks.is_empty() {
        println!("  none");
    } else {
        for chunk in result.selected_chunks.iter().take(5) {
            println!(
                "  {}: lines {}-{} | {} | score {} | tokens {}",
                crate::paths::relative_display(source, &chunk.path),
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
                crate::paths::relative_display(source, &chunk.path),
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

fn print_extraction_issues(source: &Path, issues: &[ExtractionIssue]) {
    println!("Extraction warnings: {}", issues.len());
    print_extraction_issue_details(source, issues);
}

fn print_extraction_issue_details(source: &Path, issues: &[ExtractionIssue]) {
    for issue in issues {
        eprintln!(
            "warning: skipped `{}`: {}",
            crate::paths::relative_display(source, &issue.path),
            issue.message
        );
    }
}

fn display_output_path(path: &Path) -> String {
    path.strip_prefix(".").unwrap_or(path).display().to_string()
}
