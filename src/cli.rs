use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};

use crate::{
    config::{write_default_config, DEFAULT_CONFIG_FILE},
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
