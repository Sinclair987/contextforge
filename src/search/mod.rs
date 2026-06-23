use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    chunk::ChunkKind,
    corpus::ExtractionIssue,
    index::{load_indexed_corpus, IndexRefresh},
    rank::{normalize_search_text, rank_chunks, QueryTerms, ScoreBreakdown},
    scanner::{FileKind, ScanOptions},
    Result,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub path: PathBuf,
    pub kind: ChunkKind,
    pub title: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub score: usize,
    pub token_estimate: usize,
    pub text: String,
    pub preview: String,
    pub score_breakdown: ScoreBreakdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub extraction_issues: Vec<ExtractionIssue>,
    pub index_refresh: IndexRefresh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchGroup {
    pub path: PathBuf,
    pub file_kind: FileKind,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFileType {
    Pdf,
    Docx,
    Epub,
    Markdown,
    Text,
    Code,
    Data,
    Markup,
    Config,
}

impl SearchFileType {
    fn matches(self, kind: FileKind) -> bool {
        match self {
            Self::Pdf => kind == FileKind::Pdf,
            Self::Docx => kind == FileKind::Docx,
            Self::Epub => kind == FileKind::Epub,
            Self::Markdown => kind == FileKind::Markdown,
            Self::Text => kind == FileKind::Text,
            Self::Code => matches!(kind, FileKind::Rust | FileKind::Code),
            Self::Data => matches!(kind, FileKind::Csv | FileKind::Tsv),
            Self::Markup => matches!(kind, FileKind::Xml | FileKind::Html),
            Self::Config => matches!(kind, FileKind::Toml | FileKind::Json | FileKind::Yaml),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub exact: bool,
    pub file_types: Vec<SearchFileType>,
    pub included_paths: Vec<PathBuf>,
    pub excluded_paths: Vec<PathBuf>,
}

pub fn group_hits(hits: Vec<SearchHit>, limit: usize, per_file: usize) -> Vec<SearchGroup> {
    let mut groups = Vec::<SearchGroup>::new();
    let mut group_indexes = BTreeMap::<PathBuf, usize>::new();
    let mut accepted = 0;

    for hit in hits {
        if limit > 0 && accepted >= limit {
            break;
        }
        let existing_index = group_indexes.get(&hit.path).copied();
        if existing_index.is_some_and(|index| per_file > 0 && groups[index].hits.len() >= per_file)
        {
            continue;
        }

        if let Some(index) = existing_index {
            groups[index].hits.push(hit);
        } else {
            let path = hit.path.clone();
            let index = groups.len();
            groups.push(SearchGroup {
                file_kind: FileKind::from_path(&path),
                path: path.clone(),
                hits: vec![hit],
            });
            group_indexes.insert(path, index);
        }
        accepted += 1;
    }

    groups
}

pub fn search_directory(source: &Path, query: &str) -> Result<Vec<SearchHit>> {
    search_directory_with_options(source, query, &ScanOptions::default())
}

pub fn search_directory_with_options(
    source: &Path,
    query: &str,
    scan_options: &ScanOptions,
) -> Result<Vec<SearchHit>> {
    search_directory_report_with_options(source, query, scan_options).map(|result| result.hits)
}

pub fn search_directory_report_with_options(
    source: &Path,
    query: &str,
    scan_options: &ScanOptions,
) -> Result<SearchResult> {
    search_directory_report_with_search_options(
        source,
        query,
        scan_options,
        &SearchOptions::default(),
    )
}

pub fn search_directory_report_with_search_options(
    source: &Path,
    query: &str,
    scan_options: &ScanOptions,
    search_options: &SearchOptions,
) -> Result<SearchResult> {
    let indexed = load_indexed_corpus(source, scan_options)?;
    let corpus = indexed.corpus;
    let chunks = corpus
        .chunks
        .into_iter()
        .filter(|chunk| {
            path_is_in_scope(source, &chunk.path, search_options)
                && (search_options.file_types.is_empty()
                    || search_options
                        .file_types
                        .iter()
                        .any(|file_type| file_type.matches(FileKind::from_path(&chunk.path))))
        })
        .collect::<Vec<_>>();
    let terms = QueryTerms::parse(query);
    let mut hits = if terms.is_empty() {
        Vec::new()
    } else {
        rank_chunks(chunks, &terms)
            .into_iter()
            .map(|chunk| SearchHit {
                path: chunk.path,
                kind: chunk.kind,
                title: chunk.title,
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                score: chunk.score,
                token_estimate: chunk.token_estimate,
                text: chunk.text,
                preview: chunk.preview,
                score_breakdown: chunk.score_breakdown,
            })
            .collect()
    };
    if search_options.exact {
        let phrase = normalize_search_text(query);
        hits.retain(|hit| hit_contains_phrase(source, hit, &phrase));
    }

    Ok(SearchResult {
        hits,
        extraction_issues: corpus.extraction_issues,
        index_refresh: indexed.refresh,
    })
}

fn hit_contains_phrase(source: &Path, hit: &SearchHit, phrase: &str) -> bool {
    !phrase.is_empty()
        && (normalize_search_text(&hit.text).contains(phrase)
            || hit
                .title
                .as_deref()
                .is_some_and(|title| normalize_search_text(title).contains(phrase))
            || normalize_search_text(&crate::paths::relative_display(source, &hit.path))
                .contains(phrase))
}

fn path_is_in_scope(source: &Path, path: &Path, options: &SearchOptions) -> bool {
    let source = crate::paths::absolute(source);
    let path = crate::paths::absolute(path);
    let relative = path.strip_prefix(source).unwrap_or(&path);
    let included = options.included_paths.is_empty()
        || options
            .included_paths
            .iter()
            .any(|included| relative.starts_with(included));
    let excluded = options
        .excluded_paths
        .iter()
        .any(|excluded| relative.starts_with(excluded));
    included && !excluded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rank::ScoreBreakdown;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn search_directory_returns_highest_scoring_query_match_first() {
        let temp = tempdir().expect("temporary directory");
        let root = temp.path();
        fs::create_dir_all(root.join("docs")).expect("docs directory");
        fs::write(
            root.join("docs/rust.md"),
            "# Ownership\nOwnership and borrowing are central Rust ideas.\n",
        )
        .expect("markdown file");
        fs::write(root.join("notes.txt"), "unrelated grocery list\n").expect("notes file");

        let hits = search_directory(root, "ownership borrowing").expect("hits");

        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("rust.md"));
        assert_eq!(hits[0].kind, ChunkKind::MarkdownSection);
        assert_eq!(hits[0].title.as_deref(), Some("Ownership"));
        assert_eq!(hits[0].start_line, 1);
        assert!(hits[0].score > 0);
        assert!(hits[0].preview.contains("Ownership and borrowing"));
    }

    #[test]
    fn search_directory_builds_and_reuses_local_index() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("notes.md"), "indexed local knowledge\n").expect("source file");

        let first = search_directory_report_with_options(
            temp.path(),
            "local knowledge",
            &ScanOptions::default(),
        )
        .expect("first search");
        let second = search_directory_report_with_options(
            temp.path(),
            "local knowledge",
            &ScanOptions::default(),
        )
        .expect("second search");

        assert_eq!(first.index_refresh.updated_files, 1);
        assert_eq!(second.index_refresh.reused_files, 1);
    }

    #[test]
    fn group_hits_limits_repetition_without_losing_other_files() {
        let hits = vec![
            fixture_hit("a.md", 30),
            fixture_hit("a.md", 29),
            fixture_hit("a.md", 28),
            fixture_hit("b.pdf", 27),
        ];

        let groups = group_hits(hits, 3, 2);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].hits.len(), 2);
        assert_eq!(groups[1].path, PathBuf::from("b.pdf"));
    }

    #[test]
    fn exact_search_requires_the_normalized_full_phrase() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("exact.md"), "齐泽克的哲学研究\n").expect("exact file");
        fs::write(
            temp.path().join("scattered.md"),
            "整齐的方法，泽被后世，克服困难\n",
        )
        .expect("scattered file");
        let options = SearchOptions {
            exact: true,
            ..SearchOptions::default()
        };

        let result = search_directory_report_with_search_options(
            temp.path(),
            "齐泽克",
            &ScanOptions::default(),
            &options,
        )
        .expect("exact search");

        assert_eq!(result.hits.len(), 1);
        assert!(result.hits[0].path.ends_with("exact.md"));
    }

    #[test]
    fn search_file_type_filter_excludes_other_supported_formats() {
        let temp = tempdir().expect("temporary directory");
        fs::write(temp.path().join("notes.md"), "shared knowledge\n").expect("markdown file");
        fs::write(temp.path().join("module.rs"), "fn shared_knowledge() {}\n").expect("Rust file");
        let options = SearchOptions {
            file_types: vec![SearchFileType::Markdown],
            ..SearchOptions::default()
        };

        let result = search_directory_report_with_search_options(
            temp.path(),
            "shared knowledge",
            &ScanOptions::default(),
            &options,
        )
        .expect("type-filtered search");

        assert!(!result.hits.is_empty());
        assert!(result.hits.iter().all(|hit| hit.path.ends_with("notes.md")));
    }

    #[test]
    fn search_path_filters_scope_cached_chunks_without_rebuilding_index() {
        let temp = tempdir().expect("temporary directory");
        fs::create_dir_all(temp.path().join("public")).expect("public directory");
        fs::create_dir_all(temp.path().join("private")).expect("private directory");
        fs::write(
            temp.path().join("public/notes.md"),
            "shared searchable knowledge\n",
        )
        .expect("public file");
        fs::write(
            temp.path().join("private/notes.md"),
            "shared searchable knowledge\n",
        )
        .expect("private file");
        let options = SearchOptions {
            included_paths: vec![PathBuf::from("public")],
            ..SearchOptions::default()
        };

        let result = search_directory_report_with_search_options(
            temp.path(),
            "searchable knowledge",
            &ScanOptions::default(),
            &options,
        )
        .expect("path-filtered search");

        assert!(!result.hits.is_empty());
        assert!(result
            .hits
            .iter()
            .all(|hit| hit.path.ends_with("public/notes.md")));
        assert_eq!(result.index_refresh.indexed_files, 2);
    }

    #[test]
    fn exact_search_does_not_match_the_source_directory_name() {
        let temp = tempdir().expect("temporary directory");
        let source = temp.path().join("齐泽克");
        fs::create_dir_all(&source).expect("source directory");
        fs::write(source.join("notes.md"), "整齐的方法，泽被后世，克服困难\n")
            .expect("scattered file");
        let options = SearchOptions {
            exact: true,
            ..SearchOptions::default()
        };

        let result = search_directory_report_with_search_options(
            &source,
            "齐泽克",
            &ScanOptions::default(),
            &options,
        )
        .expect("exact search");

        assert!(result.hits.is_empty());
    }

    fn fixture_hit(path: &str, score: usize) -> SearchHit {
        SearchHit {
            path: PathBuf::from(path),
            kind: ChunkKind::Paragraph,
            title: None,
            start_line: 1,
            end_line: 1,
            score,
            token_estimate: 1,
            text: "knowledge".to_string(),
            preview: "knowledge".to_string(),
            score_breakdown: ScoreBreakdown {
                lexical_score: score,
                text_match_score: 0,
                term_coverage_score: 0,
                full_coverage_score: 0,
                path_match_score: 0,
                title_match_score: 0,
                file_name_match_score: 0,
                file_kind_score: 0,
                density_score: 0,
                total_score: score,
                reasons: Vec::new(),
            },
        }
    }
}
