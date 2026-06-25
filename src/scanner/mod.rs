use std::{
    ffi::OsStr,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use crate::{ContextForgeError, Result};
use serde::{Deserialize, Serialize};

const GENERATED_OUTPUT_FILES: [&str; 3] = [
    "context-bundle.md",
    "context-manifest.json",
    "context-report.md",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Markdown,
    Rust,
    Code,
    Text,
    Toml,
    Json,
    Yaml,
    Csv,
    Tsv,
    Xml,
    Html,
    Pdf,
    Docx,
    Epub,
    Other,
}

impl FileKind {
    pub(crate) fn from_path(path: &Path) -> Self {
        if let Some(kind) = file_name_kind(path) {
            return kind;
        }

        match path
            .extension()
            .and_then(OsStr::to_str)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("md" | "markdown") => Self::Markdown,
            Some("rs") => Self::Rust,
            Some(
                "txt" | "text" | "log" | "out" | "err" | "env" | "ini" | "cfg" | "conf"
                | "properties",
            ) => Self::Text,
            Some("toml") => Self::Toml,
            Some("json") => Self::Json,
            Some("yaml" | "yml") => Self::Yaml,
            Some("csv") => Self::Csv,
            Some("tsv") => Self::Tsv,
            Some("xml" | "xsd" | "svg") => Self::Xml,
            Some("html" | "htm") => Self::Html,
            Some(
                "py" | "js" | "jsx" | "ts" | "tsx" | "java" | "c" | "h" | "cc" | "cpp" | "cxx"
                | "hpp" | "cs" | "go" | "rb" | "php" | "swift" | "kt" | "kts" | "scala" | "sh"
                | "bash" | "zsh" | "ps1" | "sql" | "lua" | "r" | "m" | "mm" | "dart" | "ex" | "exs"
                | "clj" | "cljs" | "fs" | "fsx" | "vb" | "gradle",
            ) => Self::Code,
            Some("pdf") => Self::Pdf,
            Some("docx") => Self::Docx,
            Some("epub") => Self::Epub,
            _ => Self::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Rust => "Rust",
            Self::Code => "Code",
            Self::Text => "Text",
            Self::Toml => "TOML",
            Self::Json => "JSON",
            Self::Yaml => "YAML",
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
            Self::Xml => "XML",
            Self::Html => "HTML",
            Self::Pdf => "PDF",
            Self::Docx => "DOCX",
            Self::Epub => "EPUB",
            Self::Other => "Other",
        }
    }

    fn can_be_binary_document(self) -> bool {
        matches!(self, Self::Pdf | Self::Docx | Self::Epub)
    }

    fn size_limit(self, options: &ScanOptions) -> u64 {
        if self.can_be_binary_document() {
            options.max_document_bytes
        } else {
            options.max_file_bytes
        }
    }
}

fn file_name_kind(path: &Path) -> Option<FileKind> {
    let name = path.file_name().and_then(OsStr::to_str)?;
    let lower = name.to_ascii_lowercase();

    if matches!(
        lower.as_str(),
        ".env"
            | ".env.local"
            | ".env.sample"
            | ".env.example"
            | ".gitignore"
            | ".dockerignore"
            | ".npmrc"
            | ".editorconfig"
            | "requirements.txt"
            | "yarn.lock"
            | "pnpm-lock.yaml"
    ) {
        return Some(FileKind::Text);
    }

    if matches!(name, "Cargo.lock") {
        return Some(FileKind::Toml);
    }

    if matches!(
        name,
        "Dockerfile" | "Makefile" | "Justfile" | "Rakefile" | "Gemfile" | "Jenkinsfile"
    ) {
        return Some(FileKind::Code);
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    IgnoredDirectory,
    FilteredPath,
    TooLarge,
    Binary,
}

impl SkipReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::IgnoredDirectory => "Ignored directory",
            Self::FilteredPath => "Filtered path",
            Self::TooLarge => "Too large",
            Self::Binary => "Binary",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub kind: FileKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedEntry {
    pub path: PathBuf,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions {
    pub max_file_bytes: u64,
    pub max_document_bytes: u64,
    pub pdf_timeout_seconds: u64,
    pub ignored_directories: Vec<String>,
    pub included_paths: Vec<PathBuf>,
    pub excluded_paths: Vec<PathBuf>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_file_bytes: 8 * 1024 * 1024,
            max_document_bytes: 64 * 1024 * 1024,
            pdf_timeout_seconds: 5,
            ignored_directories: vec![
                ".git".to_string(),
                "target".to_string(),
                "node_modules".to_string(),
                "dist".to_string(),
                "build".to_string(),
                "out".to_string(),
                "demo-output".to_string(),
                "venv".to_string(),
            ],
            included_paths: Vec::new(),
            excluded_paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanSummary {
    pub files: Vec<FileInfo>,
    pub skipped: Vec<SkippedEntry>,
}

impl ScanSummary {
    pub fn count_by_kind(&self, kind: FileKind) -> usize {
        self.files.iter().filter(|file| file.kind == kind).count()
    }

    pub fn count_by_skip_reason(&self, reason: SkipReason) -> usize {
        self.skipped
            .iter()
            .filter(|entry| entry.reason == reason)
            .count()
    }
}

pub fn scan_directory(source: &Path, options: &ScanOptions) -> Result<ScanSummary> {
    if !source.exists() {
        return Err(ContextForgeError::ScanSourceMissing {
            path: source.to_path_buf(),
        });
    }

    if !source.is_dir() {
        return Err(ContextForgeError::ScanSourceNotDirectory {
            path: source.to_path_buf(),
        });
    }

    let mut summary = ScanSummary::default();
    visit_directory(source, source, options, &mut summary)?;
    Ok(summary)
}

fn visit_directory(
    root: &Path,
    directory: &Path,
    options: &ScanOptions,
    summary: &mut ScanSummary,
) -> Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory).map_err(|source| ContextForgeError::ReadDirectory {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| ContextForgeError::ReadDirectoryEntry {
            path: directory.to_path_buf(),
            source,
        })?;
        entries.push(entry.path());
    }
    entries.sort();

    for path in entries {
        let metadata = fs::metadata(&path).map_err(|source| ContextForgeError::ReadMetadata {
            path: path.to_path_buf(),
            source,
        })?;
        let relative = path.strip_prefix(root).unwrap_or(&path);

        if is_excluded_path(relative, options) {
            summary.skipped.push(SkippedEntry {
                path,
                reason: SkipReason::FilteredPath,
            });
            continue;
        }

        if metadata.is_dir() {
            if !should_visit_directory(relative, options) {
                summary.skipped.push(SkippedEntry {
                    path,
                    reason: SkipReason::FilteredPath,
                });
            } else if is_ignored_directory(&path, options) {
                summary.skipped.push(SkippedEntry {
                    path,
                    reason: SkipReason::IgnoredDirectory,
                });
            } else {
                visit_directory(root, &path, options, summary)?;
            }
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        if !is_included_file(relative, options) {
            summary.skipped.push(SkippedEntry {
                path,
                reason: SkipReason::FilteredPath,
            });
            continue;
        }

        if is_generated_output_file(&path) {
            continue;
        }

        let kind = FileKind::from_path(&path);
        if metadata.len() > kind.size_limit(options) {
            summary.skipped.push(SkippedEntry {
                path,
                reason: SkipReason::TooLarge,
            });
            continue;
        }

        if !kind.can_be_binary_document() && file_starts_with_binary_content(&path, metadata.len())?
        {
            summary.skipped.push(SkippedEntry {
                path,
                reason: SkipReason::Binary,
            });
            continue;
        }

        summary.files.push(FileInfo {
            kind,
            size_bytes: metadata.len(),
            path,
        });
    }

    Ok(())
}

fn file_starts_with_binary_content(path: &Path, file_size_bytes: u64) -> Result<bool> {
    const PROBE_BYTES: u64 = 8 * 1024;

    let file = fs::File::open(path).map_err(|source| ContextForgeError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut content = Vec::with_capacity(PROBE_BYTES as usize);
    file.take(PROBE_BYTES)
        .read_to_end(&mut content)
        .map_err(|source| ContextForgeError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
    let probe_was_truncated = file_size_bytes > content.len() as u64;
    Ok(is_binary(&content, probe_was_truncated))
}

fn should_visit_directory(relative: &Path, options: &ScanOptions) -> bool {
    options.included_paths.is_empty()
        || options
            .included_paths
            .iter()
            .any(|included| included.starts_with(relative) || relative.starts_with(included))
}

fn is_included_file(relative: &Path, options: &ScanOptions) -> bool {
    options.included_paths.is_empty()
        || options
            .included_paths
            .iter()
            .any(|included| relative.starts_with(included))
}

fn is_excluded_path(relative: &Path, options: &ScanOptions) -> bool {
    options
        .excluded_paths
        .iter()
        .any(|excluded| relative.starts_with(excluded))
}

fn is_ignored_directory(path: &Path, options: &ScanOptions) -> bool {
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };

    if is_hidden_tooling_directory(name) {
        return true;
    }

    if is_contextforge_output_directory(path) {
        return true;
    }

    options
        .ignored_directories
        .iter()
        .any(|ignored| ignored == name)
}

fn is_hidden_tooling_directory(name: &str) -> bool {
    name.starts_with('.') && name != ".github"
}

fn is_contextforge_output_directory(path: &Path) -> bool {
    GENERATED_OUTPUT_FILES
        .iter()
        .filter(|file_name| path.join(file_name).is_file())
        .count()
        >= 2
}

fn is_generated_output_file(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| {
            name == crate::config::DEFAULT_CONFIG_FILE || GENERATED_OUTPUT_FILES.contains(&name)
        })
}

fn is_binary(content: &[u8], probe_was_truncated: bool) -> bool {
    if content.contains(&0) {
        return true;
    }

    match std::str::from_utf8(content) {
        Ok(_) => false,
        Err(error) => error.error_len().is_some() || !probe_was_truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scan_directory_collects_supported_text_files() {
        let temp = tempdir().expect("temporary directory");
        let root = temp.path();
        fs::create_dir_all(root.join("src")).expect("source directory");
        fs::create_dir_all(root.join("scripts")).expect("scripts directory");
        fs::write(root.join("README.md"), "# Notes\n").expect("markdown file");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("rust file");
        fs::write(root.join("scripts/build.py"), "def build():\n    pass\n").expect("python file");
        fs::write(root.join("notes.txt"), "plain notes\n").expect("text file");
        fs::write(root.join("settings.yaml"), "feature: enabled\n").expect("yaml file");
        fs::write(root.join("data.csv"), "name,value\nalpha,1\n").expect("csv file");
        fs::write(root.join("page.html"), "<h1>Notes</h1>\n").expect("html file");
        fs::write(root.join("guide.pdf"), [0_u8, 159, 146, 150]).expect("pdf file");
        fs::write(root.join("brief.docx"), [0_u8, 159, 146, 150]).expect("docx file");
        fs::write(root.join("book.epub"), [0_u8, 159, 146, 150]).expect("epub file");

        let summary = scan_directory(root, &ScanOptions::default()).expect("scan summary");

        assert_eq!(summary.files.len(), 10);
        assert_eq!(summary.count_by_kind(FileKind::Markdown), 1);
        assert_eq!(summary.count_by_kind(FileKind::Rust), 1);
        assert_eq!(summary.count_by_kind(FileKind::Code), 1);
        assert_eq!(summary.count_by_kind(FileKind::Text), 1);
        assert_eq!(summary.count_by_kind(FileKind::Yaml), 1);
        assert_eq!(summary.count_by_kind(FileKind::Csv), 1);
        assert_eq!(summary.count_by_kind(FileKind::Html), 1);
        assert_eq!(summary.count_by_kind(FileKind::Pdf), 1);
        assert_eq!(summary.count_by_kind(FileKind::Docx), 1);
        assert_eq!(summary.count_by_kind(FileKind::Epub), 1);
    }

    #[test]
    fn scan_directory_applies_a_separate_document_size_limit() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("large.txt"), "123456789").expect("text file");
        fs::write(temp.path().join("book.pdf"), [0_u8; 9]).expect("PDF file");
        let options = ScanOptions {
            max_file_bytes: 4,
            max_document_bytes: 16,
            ..ScanOptions::default()
        };

        let summary = scan_directory(temp.path(), &options).expect("scan summary");

        assert_eq!(summary.count_by_kind(FileKind::Pdf), 1);
        assert_eq!(summary.count_by_skip_reason(SkipReason::TooLarge), 1);
    }

    #[test]
    fn scan_directory_records_skipped_entries() {
        let temp = tempdir().expect("temporary directory");
        let root = temp.path();
        fs::create_dir_all(root.join("target")).expect("target directory");
        fs::write(root.join("target/generated.rs"), "fn generated() {}\n").expect("ignored file");
        fs::write(root.join("image.bin"), [0_u8, 159, 146, 150]).expect("binary file");
        fs::write(root.join("large.txt"), "123456789").expect("large file");

        let options = ScanOptions {
            max_file_bytes: 4,
            ..ScanOptions::default()
        };
        let summary = scan_directory(root, &options).expect("scan summary");

        assert_eq!(
            summary.count_by_skip_reason(SkipReason::IgnoredDirectory),
            1
        );
        assert_eq!(summary.count_by_skip_reason(SkipReason::Binary), 1);
        assert_eq!(summary.count_by_skip_reason(SkipReason::TooLarge), 1);
    }

    #[test]
    fn scan_directory_accepts_text_when_probe_ends_inside_utf8_character() {
        let temp = tempdir().expect("temporary directory");
        let root = temp.path();
        let content = format!("{}中\n", "a".repeat(8 * 1024 - 1));
        fs::write(root.join("notes.md"), content).expect("markdown file");

        let summary = scan_directory(root, &ScanOptions::default()).expect("scan summary");

        assert_eq!(summary.count_by_kind(FileKind::Markdown), 1);
        assert_eq!(summary.count_by_skip_reason(SkipReason::Binary), 0);
    }

    #[test]
    fn scan_directory_ignores_local_tooling_directories_by_default() {
        let temp = tempdir().expect("temporary directory");
        let root = temp.path();
        for directory in [
            ".workspace-cache",
            ".deps",
            ".tool-state",
            "out",
            "demo-output",
        ] {
            fs::create_dir_all(root.join(directory)).expect("tool directory");
            fs::write(root.join(directory).join("notes.md"), "tool notes\n").expect("tool file");
        }
        fs::write(root.join("README.md"), "# Project\n").expect("project file");

        let summary = scan_directory(root, &ScanOptions::default()).expect("scan summary");

        assert_eq!(summary.files.len(), 1);
        assert!(summary.files[0].path.ends_with("README.md"));
        assert_eq!(
            summary.count_by_skip_reason(SkipReason::IgnoredDirectory),
            5
        );
    }
}
