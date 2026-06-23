use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{
    chunk::Chunk,
    corpus::{extract_files, Corpus, ExtractionIssue},
    scanner::{scan_directory, FileInfo, FileKind, ScanOptions},
    ContextForgeError, Result,
};

const INDEX_SCHEMA_VERSION: u32 = 1;
const INDEX_COMPATIBILITY_VERSION: u32 = 1;
const TOOLING_DIRECTORY: &str = ".contextforge";
const INDEX_FILE: &str = "index-v1.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct KnowledgeIndex {
    schema_version: u32,
    compatibility_version: u32,
    scanner_fingerprint: String,
    updated_unix_seconds: u64,
    files: BTreeMap<String, IndexedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct IndexedFile {
    size_bytes: u64,
    modified_unix_nanos: u64,
    kind: FileKind,
    chunks: Vec<Chunk>,
    issue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexRefresh {
    pub path: PathBuf,
    pub created: bool,
    pub rebuilt: bool,
    pub reused_files: usize,
    pub updated_files: usize,
    pub removed_files: usize,
    pub indexed_files: usize,
    pub indexed_chunks: usize,
    pub warning_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexState {
    Missing,
    Fresh,
    Stale,
    Corrupt,
    Incompatible,
}

impl IndexState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Corrupt => "corrupt",
            Self::Incompatible => "incompatible",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexStatus {
    pub path: PathBuf,
    pub state: IndexState,
    pub schema_version: Option<u32>,
    pub indexed_files: usize,
    pub indexed_chunks: usize,
    pub updated_unix_seconds: Option<u64>,
    pub warnings: Vec<(PathBuf, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedCorpus {
    pub corpus: Corpus,
    pub refresh: IndexRefresh,
}

struct RefreshedIndex {
    index: KnowledgeIndex,
    scan: crate::scanner::ScanSummary,
    refresh: IndexRefresh,
}

fn index_path(source: &Path) -> PathBuf {
    source.join(TOOLING_DIRECTORY).join(INDEX_FILE)
}

fn load_index(source: &Path) -> Result<Option<KnowledgeIndex>> {
    let path = index_path(source);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).map_err(|source| ContextForgeError::ReadIndex {
        path: path.clone(),
        source,
    })?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|source| ContextForgeError::ParseIndex { path, source })
}

fn save_index(source: &Path, index: &KnowledgeIndex) -> Result<()> {
    let path = index_path(source);
    let directory = path.parent().unwrap_or(source);
    fs::create_dir_all(directory).map_err(|source| ContextForgeError::WriteIndex {
        path: path.clone(),
        source,
    })?;
    fs::write(directory.join(".gitignore"), "*\n").map_err(|source| {
        ContextForgeError::WriteIndex {
            path: path.clone(),
            source,
        }
    })?;

    let mut temporary =
        NamedTempFile::new_in(directory).map_err(|source| ContextForgeError::WriteIndex {
            path: path.clone(),
            source,
        })?;
    serde_json::to_writer_pretty(temporary.as_file_mut(), index)
        .map_err(|source| ContextForgeError::SerializeIndex { source })?;
    temporary
        .write_all(b"\n")
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| ContextForgeError::WriteIndex {
            path: path.clone(),
            source,
        })?;
    temporary
        .persist(&path)
        .map_err(|error| ContextForgeError::WriteIndex {
            path,
            source: error.error,
        })?;
    Ok(())
}

pub fn refresh_index(source: &Path, options: &ScanOptions, force: bool) -> Result<IndexRefresh> {
    refresh_index_data(source, options, force).map(|refreshed| refreshed.refresh)
}

fn refresh_index_data(source: &Path, options: &ScanOptions, force: bool) -> Result<RefreshedIndex> {
    let scan = scan_directory(source, options)?;
    let scanner_fingerprint = scanner_fingerprint(options)?;
    let index_existed = index_path(source).exists();
    let (existing, corrupt) = match load_index(source) {
        Ok(index) => (index, false),
        Err(ContextForgeError::ParseIndex { .. }) => (None, true),
        Err(error) => return Err(error),
    };
    let created = !index_existed;
    let compatible = existing.as_ref().is_some_and(|index| {
        index.schema_version == INDEX_SCHEMA_VERSION
            && index.compatibility_version == INDEX_COMPATIBILITY_VERSION
            && index.scanner_fingerprint == scanner_fingerprint
    });
    let previous_updated_unix_seconds = existing
        .as_ref()
        .map(|index| index.updated_unix_seconds)
        .unwrap_or_default();
    let rebuilt = force || corrupt || (!created && !compatible);
    let mut previous_files = if compatible && !force {
        existing.map_or_else(BTreeMap::new, |index| index.files)
    } else {
        BTreeMap::new()
    };
    let mut indexed_files = BTreeMap::new();
    let mut changed_files = Vec::new();
    let mut reused_files = 0;

    for file in &scan.files {
        let key = crate::paths::relative_display(source, &file.path);
        let modified_unix_nanos = modified_unix_nanos(file)?;
        let unchanged = previous_files.get(&key).is_some_and(|indexed| {
            indexed.size_bytes == file.size_bytes
                && indexed.modified_unix_nanos == modified_unix_nanos
                && indexed.kind == file.kind
        });
        if unchanged {
            if let Some(indexed) = previous_files.remove(&key) {
                indexed_files.insert(key, indexed);
                reused_files += 1;
            }
        } else {
            previous_files.remove(&key);
            changed_files.push(file.clone());
        }
    }

    let updated_files = changed_files.len();
    for mut extracted in extract_files(&changed_files, options.pdf_timeout_seconds) {
        let key = crate::paths::relative_display(source, &extracted.file.path);
        for chunk in &mut extracted.chunks {
            chunk.path = PathBuf::from(&key);
        }
        indexed_files.insert(
            key,
            IndexedFile {
                size_bytes: extracted.file.size_bytes,
                modified_unix_nanos: modified_unix_nanos(&extracted.file)?,
                kind: extracted.file.kind,
                chunks: extracted.chunks,
                issue: extracted.issue.map(|issue| issue.message),
            },
        );
    }

    let removed_files = previous_files.len();
    let indexed_chunks = indexed_files.values().map(|file| file.chunks.len()).sum();
    let warning_count = indexed_files
        .values()
        .filter(|file| file.issue.is_some())
        .count();
    let indexed_file_count = indexed_files.len();
    let should_write = created || rebuilt || updated_files > 0 || removed_files > 0;
    let index = KnowledgeIndex {
        schema_version: INDEX_SCHEMA_VERSION,
        compatibility_version: INDEX_COMPATIBILITY_VERSION,
        scanner_fingerprint,
        updated_unix_seconds: if should_write {
            unix_seconds()
        } else {
            previous_updated_unix_seconds
        },
        files: indexed_files,
    };
    if should_write {
        save_index(source, &index)?;
    }

    let refresh = IndexRefresh {
        path: index_path(source),
        created,
        rebuilt,
        reused_files,
        updated_files,
        removed_files,
        indexed_files: indexed_file_count,
        indexed_chunks,
        warning_count,
    };
    Ok(RefreshedIndex {
        index,
        scan,
        refresh,
    })
}

pub fn load_indexed_corpus(source: &Path, options: &ScanOptions) -> Result<IndexedCorpus> {
    let refreshed = refresh_index_data(source, options, false)?;
    let mut chunks = Vec::new();
    let mut extraction_issues = Vec::new();

    for (relative_path, indexed) in refreshed.index.files {
        let path = source.join(&relative_path);
        chunks.extend(indexed.chunks.into_iter().map(|mut chunk| {
            chunk.path = path.clone();
            chunk
        }));
        if let Some(message) = indexed.issue {
            extraction_issues.push(ExtractionIssue { path, message });
        }
    }

    Ok(IndexedCorpus {
        corpus: Corpus {
            scan: refreshed.scan,
            chunks,
            privacy_findings: Vec::new(),
            extraction_issues,
        },
        refresh: refreshed.refresh,
    })
}

pub fn index_status(source: &Path, options: &ScanOptions) -> Result<IndexStatus> {
    let path = index_path(source);
    let index = match load_index(source) {
        Ok(Some(index)) => index,
        Ok(None) => return Ok(empty_status(path, IndexState::Missing)),
        Err(ContextForgeError::ParseIndex { .. }) => {
            return Ok(empty_status(path, IndexState::Corrupt));
        }
        Err(error) => return Err(error),
    };
    let fingerprint = scanner_fingerprint(options)?;
    let compatible = index.schema_version == INDEX_SCHEMA_VERSION
        && index.compatibility_version == INDEX_COMPATIBILITY_VERSION
        && index.scanner_fingerprint == fingerprint;
    let state = if compatible && index_matches_scan(source, options, &index)? {
        IndexState::Fresh
    } else if compatible {
        IndexState::Stale
    } else {
        IndexState::Incompatible
    };
    let indexed_chunks = index.files.values().map(|file| file.chunks.len()).sum();
    let warnings = index
        .files
        .iter()
        .filter_map(|(path, file)| {
            file.issue
                .as_ref()
                .map(|issue| (PathBuf::from(path), issue.clone()))
        })
        .collect();

    Ok(IndexStatus {
        path,
        state,
        schema_version: Some(index.schema_version),
        indexed_files: index.files.len(),
        indexed_chunks,
        updated_unix_seconds: Some(index.updated_unix_seconds),
        warnings,
    })
}

pub fn clear_index(source: &Path) -> Result<bool> {
    let path = index_path(source);
    let directory = source.join(TOOLING_DIRECTORY);
    let mut removed = false;
    if path.exists() {
        fs::remove_file(&path).map_err(|source| ContextForgeError::WriteIndex {
            path: path.clone(),
            source,
        })?;
        removed = true;
    }
    let ignore_path = directory.join(".gitignore");
    if ignore_path.exists() {
        fs::remove_file(&ignore_path).map_err(|source| ContextForgeError::WriteIndex {
            path: ignore_path,
            source,
        })?;
    }
    if directory.exists() {
        match fs::remove_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
            Err(source) => {
                return Err(ContextForgeError::WriteIndex {
                    path: directory,
                    source,
                });
            }
        }
    }
    Ok(removed)
}

fn empty_status(path: PathBuf, state: IndexState) -> IndexStatus {
    IndexStatus {
        path,
        state,
        schema_version: None,
        indexed_files: 0,
        indexed_chunks: 0,
        updated_unix_seconds: None,
        warnings: Vec::new(),
    }
}

fn index_matches_scan(
    source: &Path,
    options: &ScanOptions,
    index: &KnowledgeIndex,
) -> Result<bool> {
    let scan = scan_directory(source, options)?;
    if scan.files.len() != index.files.len() {
        return Ok(false);
    }
    for file in &scan.files {
        let key = crate::paths::relative_display(source, &file.path);
        let Some(indexed) = index.files.get(&key) else {
            return Ok(false);
        };
        if indexed.size_bytes != file.size_bytes
            || indexed.modified_unix_nanos != modified_unix_nanos(file)?
            || indexed.kind != file.kind
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn scanner_fingerprint(options: &ScanOptions) -> Result<String> {
    serde_json::to_string(&(
        options.max_file_bytes,
        options.max_document_bytes,
        options.pdf_timeout_seconds,
        &options.ignored_directories,
        &options.included_paths,
        &options.excluded_paths,
    ))
    .map_err(|source| ContextForgeError::SerializeIndex { source })
}

fn modified_unix_nanos(file: &FileInfo) -> Result<u64> {
    let metadata = fs::metadata(&file.path).map_err(|source| ContextForgeError::ReadMetadata {
        path: file.path.clone(),
        source,
    })?;
    let modified = metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(u64::try_from(modified).unwrap_or(u64::MAX))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::tempdir;

    use super::*;
    use crate::{
        chunk::{Chunk, ChunkKind},
        scanner::{FileKind, ScanOptions},
    };

    #[test]
    fn index_round_trip_preserves_relative_chunks() {
        let temp = tempdir().expect("temporary directory");
        let index = KnowledgeIndex {
            schema_version: 1,
            compatibility_version: 1,
            scanner_fingerprint: "fixture".to_string(),
            updated_unix_seconds: 1,
            files: BTreeMap::from([(
                "docs/notes.md".to_string(),
                IndexedFile {
                    size_bytes: 17,
                    modified_unix_nanos: 1,
                    kind: FileKind::Markdown,
                    chunks: vec![Chunk {
                        path: PathBuf::from("docs/notes.md"),
                        kind: ChunkKind::Paragraph,
                        title: None,
                        start_line: 1,
                        end_line: 1,
                        text: "indexed knowledge".to_string(),
                        token_estimate: 5,
                    }],
                    issue: None,
                },
            )]),
        };

        save_index(temp.path(), &index).expect("save index");
        let loaded = load_index(temp.path())
            .expect("load index")
            .expect("index exists");

        assert_eq!(loaded, index);
        assert_eq!(
            index_path(temp.path()),
            temp.path().join(".contextforge/index-v1.json")
        );
        assert_eq!(
            fs::read_to_string(temp.path().join(".contextforge/.gitignore"))
                .expect("tooling ignore file"),
            "*\n"
        );
    }

    #[test]
    fn refresh_index_creates_records_for_scanned_files() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("alpha.md"), "alpha knowledge\n").expect("alpha file");
        fs::write(temp.path().join("beta.txt"), "beta knowledge\n").expect("beta file");

        let refresh = refresh_index(temp.path(), &ScanOptions::default(), false)
            .expect("initial index refresh");

        assert_eq!(refresh.updated_files, 2);
        assert_eq!(refresh.reused_files, 0);
        assert_eq!(refresh.indexed_files, 2);
        assert!(index_path(temp.path()).is_file());
    }

    #[test]
    fn refresh_index_reuses_unchanged_files() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("notes.md"), "stable knowledge\n").expect("source file");
        refresh_index(temp.path(), &ScanOptions::default(), false).expect("initial refresh");

        let refresh =
            refresh_index(temp.path(), &ScanOptions::default(), false).expect("unchanged refresh");

        assert_eq!(refresh.updated_files, 0);
        assert_eq!(refresh.reused_files, 1);
    }

    #[test]
    fn refresh_index_replaces_changed_file_without_counting_it_as_removed() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join("notes.md");
        fs::write(&path, "short knowledge\n").expect("initial source");
        refresh_index(temp.path(), &ScanOptions::default(), false).expect("initial refresh");
        fs::write(&path, "longer changed knowledge\n").expect("changed source");

        let refresh =
            refresh_index(temp.path(), &ScanOptions::default(), false).expect("changed refresh");

        assert_eq!(refresh.updated_files, 1);
        assert_eq!(refresh.removed_files, 0);
    }

    #[test]
    fn refresh_index_rebuilds_corrupt_index() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("notes.md"), "recoverable knowledge\n").expect("source file");
        fs::create_dir_all(temp.path().join(".contextforge")).expect("index directory");
        fs::write(index_path(temp.path()), "{not-json").expect("corrupt index");

        let refresh = refresh_index(temp.path(), &ScanOptions::default(), false)
            .expect("corrupt index rebuild");

        assert!(refresh.rebuilt);
        assert_eq!(refresh.updated_files, 1);
    }

    #[test]
    fn refresh_index_does_not_rewrite_unchanged_index() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("notes.md"), "stable indexed knowledge\n").expect("source file");
        refresh_index(temp.path(), &ScanOptions::default(), false).expect("initial refresh");
        let mut index = load_index(temp.path())
            .expect("load index")
            .expect("index exists");
        index.updated_unix_seconds = 42;
        save_index(temp.path(), &index).expect("save fixed timestamp");

        refresh_index(temp.path(), &ScanOptions::default(), false).expect("unchanged refresh");
        let refreshed = load_index(temp.path())
            .expect("load refreshed index")
            .expect("index exists");

        assert_eq!(refreshed.updated_unix_seconds, 42);
    }

    #[test]
    fn refresh_index_removes_deleted_files() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join("notes.md");
        fs::write(&path, "temporary indexed knowledge\n").expect("source file");
        refresh_index(temp.path(), &ScanOptions::default(), false).expect("initial refresh");
        fs::remove_file(path).expect("delete source");

        let refresh = refresh_index(temp.path(), &ScanOptions::default(), false)
            .expect("deleted-file refresh");

        assert_eq!(refresh.removed_files, 1);
        assert_eq!(refresh.indexed_files, 0);
    }

    #[test]
    fn refresh_index_adds_new_files_without_rebuilding_existing_records() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("first.md"), "first indexed knowledge\n")
            .expect("first source file");
        refresh_index(temp.path(), &ScanOptions::default(), false).expect("initial refresh");
        fs::write(temp.path().join("second.md"), "second indexed knowledge\n")
            .expect("second source file");

        let refresh =
            refresh_index(temp.path(), &ScanOptions::default(), false).expect("added-file refresh");

        assert_eq!(refresh.updated_files, 1);
        assert_eq!(refresh.reused_files, 1);
        assert_eq!(refresh.indexed_files, 2);
        assert!(!refresh.rebuilt);
    }

    #[test]
    fn refresh_index_reuses_unchanged_extraction_failure() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("broken.epub"), "not a zip archive").expect("broken EPUB");

        let first = refresh_index(temp.path(), &ScanOptions::default(), false)
            .expect("initial failed extraction");
        let second = refresh_index(temp.path(), &ScanOptions::default(), false)
            .expect("reused failed extraction");

        assert_eq!(first.warning_count, 1);
        assert_eq!(second.updated_files, 0);
        assert_eq!(second.reused_files, 1);
        assert_eq!(second.warning_count, 1);
    }

    #[test]
    fn refresh_index_retries_extraction_failure_after_file_changes() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join("broken.epub");
        fs::write(&path, "not a zip archive").expect("broken EPUB");
        refresh_index(temp.path(), &ScanOptions::default(), false)
            .expect("initial failed extraction");
        fs::write(&path, "still not a zip archive, but changed").expect("changed broken EPUB");

        let refresh = refresh_index(temp.path(), &ScanOptions::default(), false)
            .expect("changed failed extraction");

        assert_eq!(refresh.updated_files, 1);
        assert_eq!(refresh.reused_files, 0);
        assert_eq!(refresh.warning_count, 1);
    }

    #[test]
    fn refresh_index_rebuilds_when_scanner_configuration_changes() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("notes.md"), "config-sensitive knowledge\n")
            .expect("source file");
        refresh_index(temp.path(), &ScanOptions::default(), false).expect("initial refresh");
        let mut changed_options = ScanOptions::default();
        changed_options.max_file_bytes += 1;

        let refresh =
            refresh_index(temp.path(), &changed_options, false).expect("configuration rebuild");

        assert!(refresh.rebuilt);
        assert_eq!(refresh.updated_files, 1);
        assert_eq!(refresh.reused_files, 0);
    }
}
