use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};

use crate::{
    audit::{audit_directory, PrivacyFinding},
    config::{write_default_config, DEFAULT_CONFIG_FILE},
    pack::{pack_directory, PackResult, BUNDLE_FILE, MANIFEST_FILE, REPORT_FILE},
    scanner::{scan_directory, FileKind, ScanOptions, ScanSummary, SkipReason},
    search::{search_directory, SearchHit},
    Result,
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
    },
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
        Commands::Audit { source } => audit_source_directory(&source),
        Commands::Pack {
            source,
            goal,
            budget,
        } => pack_source_directory(&source, &goal, budget),
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
            "{rank}. {}: line {} | score {}",
            hit.path.display(),
            hit.start_line,
            hit.score
        );
        println!("   {}", hit.preview);
    }
}

fn audit_source_directory(source: &Path) -> Result<()> {
    let findings = audit_directory(source)?;
    print_audit_findings(&findings);
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

fn pack_source_directory(source: &Path, goal: &str, budget: usize) -> Result<()> {
    let result = pack_directory(source, goal, budget, Path::new("."))?;
    print_pack_result(&result);
    Ok(())
}

fn print_pack_result(result: &PackResult) {
    println!("Generated {BUNDLE_FILE}");
    println!("Generated {MANIFEST_FILE}");
    println!("Generated {REPORT_FILE}");
    println!("Selected chunks: {}", result.selected_chunks.len());
    println!("Used tokens: {}", result.used_tokens);
    println!("Privacy findings: {}", result.privacy_findings.len());
}
