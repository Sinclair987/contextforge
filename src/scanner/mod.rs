use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use crate::{ContextForgeError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Markdown,
    Rust,
    Text,
    Toml,
    Json,
    Other,
}

impl FileKind {
    fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(OsStr::to_str)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("md" | "markdown") => Self::Markdown,
            Some("rs") => Self::Rust,
            Some("txt" | "text") => Self::Text,
            Some("toml") => Self::Toml,
            Some("json") => Self::Json,
            _ => Self::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Rust => "Rust",
            Self::Text => "Text",
            Self::Toml => "TOML",
            Self::Json => "JSON",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    IgnoredDirectory,
    TooLarge,
    Binary,
}

impl SkipReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::IgnoredDirectory => "Ignored directory",
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
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_file_bytes: 1_048_576,
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
    visit_directory(source, options, &mut summary)?;
    Ok(summary)
}

fn visit_directory(
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

        if metadata.is_dir() {
            if is_ignored_directory(&path) {
                summary.skipped.push(SkippedEntry {
                    path,
                    reason: SkipReason::IgnoredDirectory,
                });
            } else {
                visit_directory(&path, options, summary)?;
            }
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        if metadata.len() > options.max_file_bytes {
            summary.skipped.push(SkippedEntry {
                path,
                reason: SkipReason::TooLarge,
            });
            continue;
        }

        let content = fs::read(&path).map_err(|source| ContextForgeError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;

        if is_binary(&content) {
            summary.skipped.push(SkippedEntry {
                path,
                reason: SkipReason::Binary,
            });
            continue;
        }

        summary.files.push(FileInfo {
            kind: FileKind::from_path(&path),
            size_bytes: metadata.len(),
            path,
        });
    }

    Ok(())
}

fn is_ignored_directory(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(OsStr::to_str),
        Some(".git" | "target" | "node_modules")
    )
}

fn is_binary(content: &[u8]) -> bool {
    content.contains(&0) || std::str::from_utf8(content).is_err()
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
        fs::write(root.join("README.md"), "# Notes\n").expect("markdown file");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("rust file");
        fs::write(root.join("notes.txt"), "plain notes\n").expect("text file");

        let summary = scan_directory(root, &ScanOptions::default()).expect("scan summary");

        assert_eq!(summary.files.len(), 3);
        assert_eq!(summary.count_by_kind(FileKind::Markdown), 1);
        assert_eq!(summary.count_by_kind(FileKind::Rust), 1);
        assert_eq!(summary.count_by_kind(FileKind::Text), 1);
    }

    #[test]
    fn scan_directory_records_skipped_entries() {
        let temp = tempdir().expect("temporary directory");
        let root = temp.path();
        fs::create_dir_all(root.join("target")).expect("target directory");
        fs::write(root.join("target/generated.rs"), "fn generated() {}\n").expect("ignored file");
        fs::write(root.join("image.bin"), [0_u8, 159, 146, 150]).expect("binary file");
        fs::write(root.join("large.txt"), "123456789").expect("large file");

        let options = ScanOptions { max_file_bytes: 4 };
        let summary = scan_directory(root, &options).expect("scan summary");

        assert_eq!(
            summary.count_by_skip_reason(SkipReason::IgnoredDirectory),
            1
        );
        assert_eq!(summary.count_by_skip_reason(SkipReason::Binary), 1);
        assert_eq!(summary.count_by_skip_reason(SkipReason::TooLarge), 1);
    }
}
