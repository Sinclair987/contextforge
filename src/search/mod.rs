use std::path::{Path, PathBuf};

use crate::{
    chunk::{split_document, Chunk},
    extract::{Extractor, TextExtractor},
    scanner::{scan_directory, ScanOptions},
    Result,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub path: PathBuf,
    pub start_line: usize,
    pub score: usize,
    pub preview: String,
}

pub fn search_directory(source: &Path, query: &str) -> Result<Vec<SearchHit>> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let scan = scan_directory(source, &ScanOptions::default())?;
    let extractor = TextExtractor;
    let mut hits = Vec::new();

    for file in &scan.files {
        if !extractor.supports(file) {
            continue;
        }

        let document = extractor.extract(file)?;
        for chunk in split_document(&document) {
            let score = score_chunk(&chunk, &terms);
            if score == 0 {
                continue;
            }

            hits.push(SearchHit {
                path: chunk.path,
                start_line: chunk.start_line,
                score,
                preview: preview(&chunk.text),
            });
        }
    }

    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.start_line.cmp(&right.start_line))
    });

    Ok(hits)
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect()
}

fn score_chunk(chunk: &Chunk, terms: &[String]) -> usize {
    let text = chunk.text.to_ascii_lowercase();
    let path = chunk.path.to_string_lossy().to_ascii_lowercase();

    terms
        .iter()
        .map(|term| {
            let text_matches = text.matches(term).count() * 3;
            let path_matches = path.matches(term).count();
            text_matches + path_matches
        })
        .sum()
}

fn preview(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_PREVIEW_CHARS: usize = 160;
    if collapsed.chars().count() <= MAX_PREVIEW_CHARS {
        return collapsed;
    }

    collapsed.chars().take(MAX_PREVIEW_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(hits[0].start_line, 1);
        assert!(hits[0].score > 0);
        assert!(hits[0].preview.contains("Ownership and borrowing"));
    }
}
