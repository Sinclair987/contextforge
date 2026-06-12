use std::path::{Path, PathBuf};

use crate::{
    chunk::ChunkKind,
    rank::{rank_directory, ScoreBreakdown},
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

pub fn search_directory(source: &Path, query: &str) -> Result<Vec<SearchHit>> {
    rank_directory(source, query).map(|ranked| {
        ranked
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
    })
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
        assert_eq!(hits[0].kind, ChunkKind::MarkdownSection);
        assert_eq!(hits[0].title.as_deref(), Some("Ownership"));
        assert_eq!(hits[0].start_line, 1);
        assert!(hits[0].score > 0);
        assert!(hits[0].preview.contains("Ownership and borrowing"));
    }
}
