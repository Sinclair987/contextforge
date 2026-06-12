use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use crate::{
    audit::{audit_directory, PrivacyFinding, Severity},
    config::{write_default_config, DEFAULT_CONFIG_FILE},
    pack::{
        pack_directory_with_options, PackOptions, PackResult, BUNDLE_FILE, MANIFEST_FILE,
        REPORT_FILE,
    },
    scanner::{scan_directory, FileKind, ScanOptions, ScanSummary, SkipReason},
    search::{search_directory, SearchHit},
    ContextForgeError, Result,
};

#[derive(Debug, Parser)]
#[command(name = "contextforge")]
#[command(about = "Compile local project context into auditable AI-ready bundles")]
pub struct Cli {
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

    /// Search local text files for relevant context.
    Search {
        /// Directory to search.
        #[arg(long)]
        source: PathBuf,

        /// Search query.
        query: String,
    },

    /// Audit local text files for privacy risks.
    Audit {
        /// Directory to audit.
        #[arg(long)]
        source: PathBuf,

        /// Output format.
        #[arg(long, value_enum, default_value_t = AuditFormat::Text)]
        format: AuditFormat,
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

        /// Replace selected sensitive lines with redaction markers.
        #[arg(long)]
        redact: bool,

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
        Commands::Scan { source } => scan_source_directory(&source),
        Commands::Search { source, query } => search_source_directory(&source, &query),
        Commands::Audit { source, format } => audit_source_directory(&source, format),
        Commands::Pack {
            source,
            goal,
            budget,
            redact,
            fail_on,
        } => pack_source_directory(&source, &goal, budget, redact, fail_on.map(Severity::from)),
    }
}

fn init_current_directory() -> Result<()> {
    let path = PathBuf::from(DEFAULT_CONFIG_FILE);
    write_default_config(&path)?;
    println!("Created {}", path.display());
    Ok(())
}

fn scan_source_directory(source: &Path) -> Result<()> {
    let summary = scan_directory(source, &ScanOptions::default())?;
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
    print_kind_count(summary, FileKind::Text);
    print_kind_count(summary, FileKind::Toml);
    print_kind_count(summary, FileKind::Json);
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

fn search_source_directory(source: &Path, query: &str) -> Result<()> {
    let hits = search_directory(source, query)?;
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

fn audit_source_directory(source: &Path, format: AuditFormat) -> Result<()> {
    let findings = audit_directory(source)?;
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

fn pack_source_directory(
    source: &Path,
    goal: &str,
    budget: usize,
    redact: bool,
    fail_on: Option<Severity>,
) -> Result<()> {
    let result = pack_directory_with_options(
        source,
        goal,
        budget,
        Path::new("."),
        PackOptions { redact, fail_on },
    )?;
    print_pack_result(&result);
    Ok(())
}

fn print_pack_result(result: &PackResult) {
    println!("Generated {BUNDLE_FILE}");
    println!("Generated {MANIFEST_FILE}");
    println!("Generated {REPORT_FILE}");
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
