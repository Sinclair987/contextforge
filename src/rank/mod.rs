use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    chunk::{split_document, Chunk, ChunkKind},
    extract::{Extractor, TextExtractor},
    scanner::{scan_directory, ScanOptions},
    Result,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTerms {
    terms: Vec<String>,
}

impl QueryTerms {
    pub fn parse(query: &str) -> Self {
        let mut terms = Vec::new();

        for raw in query.split(|ch: char| !ch.is_alphanumeric() && ch != '_') {
            let term = raw.trim().to_ascii_lowercase();
            if term.is_empty() || terms.contains(&term) {
                continue;
            }
            terms.push(term);
        }

        Self { terms }
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn as_slice(&self) -> &[String] {
        &self.terms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoringProfile {
    pub text_match_weight: usize,
    pub path_match_weight: usize,
    pub title_match_weight: usize,
    pub file_name_match_weight: usize,
    pub density_weight: usize,
}

impl Default for ScoringProfile {
    fn default() -> Self {
        Self {
            text_match_weight: 3,
            path_match_weight: 2,
            title_match_weight: 4,
            file_name_match_weight: 3,
            density_weight: 2,
        }
    }
}

pub trait ChunkScorer {
    fn score(&self, chunk: &Chunk, terms: &QueryTerms) -> Option<ScoreBreakdown>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DefaultScorer {
    profile: ScoringProfile,
}

impl DefaultScorer {
    pub fn new(profile: ScoringProfile) -> Self {
        Self { profile }
    }
}

impl ChunkScorer for DefaultScorer {
    fn score(&self, chunk: &Chunk, terms: &QueryTerms) -> Option<ScoreBreakdown> {
        if terms.is_empty() {
            return None;
        }

        let text = chunk.text.to_ascii_lowercase();
        let path = chunk.path.to_string_lossy().to_ascii_lowercase();
        let title = chunk
            .title
            .as_deref()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let file_name = chunk
            .path
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        let text_matches = count_matches(&text, terms.as_slice());
        let path_matches = count_matches(&path, terms.as_slice());
        let title_matches = count_matches(&title, terms.as_slice());
        let file_name_matches = count_matches(&file_name, terms.as_slice());
        let total_matches = text_matches + path_matches + title_matches + file_name_matches;

        if total_matches == 0 {
            return None;
        }

        let text_match_score = text_matches * self.profile.text_match_weight;
        let path_match_score = path_matches * self.profile.path_match_weight;
        let title_match_score = title_matches * self.profile.title_match_weight;
        let file_name_match_score = file_name_matches * self.profile.file_name_match_weight;
        let chunk_kind_score = chunk_kind_bonus(chunk.kind);
        let file_kind_score = file_kind_bonus(&chunk.path) + chunk_kind_score;
        let density_score = density_bonus(text_matches, terms.as_slice().len(), self.profile);
        let total_score = text_match_score
            + path_match_score
            + title_match_score
            + file_name_match_score
            + file_kind_score
            + density_score;

        let mut reasons = Vec::new();
        if text_matches > 0 {
            reasons.push(format!(
                "text matches: {text_matches} x {}",
                self.profile.text_match_weight
            ));
        }
        if path_matches > 0 {
            reasons.push(format!(
                "path matches: {path_matches} x {}",
                self.profile.path_match_weight
            ));
        }
        if title_matches > 0 {
            reasons.push(format!(
                "title matches: {title_matches} x {}",
                self.profile.title_match_weight
            ));
        }
        if file_name_matches > 0 {
            reasons.push(format!(
                "file name matches: {file_name_matches} x {}",
                self.profile.file_name_match_weight
            ));
        }
        if file_kind_score > 0 {
            reasons.push(format!("file kind bonus: {file_kind_score}"));
        }
        if density_score > 0 {
            reasons.push(format!("term density bonus: {density_score}"));
        }

        Some(ScoreBreakdown {
            text_match_score,
            path_match_score,
            title_match_score,
            file_name_match_score,
            file_kind_score,
            density_score,
            total_score,
            reasons,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScoreBreakdown {
    pub text_match_score: usize,
    pub path_match_score: usize,
    pub title_match_score: usize,
    pub file_name_match_score: usize,
    pub file_kind_score: usize,
    pub density_score: usize,
    pub total_score: usize,
    pub reasons: Vec<String>,
}

impl ScoreBreakdown {
    pub fn summary(&self) -> String {
        if self.reasons.is_empty() {
            return format!("score {}", self.total_score);
        }

        self.reasons.join("; ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedChunk {
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

pub fn rank_directory(source: &Path, query: &str) -> Result<Vec<RankedChunk>> {
    rank_directory_with_options(source, query, &ScanOptions::default())
}

pub fn rank_directory_with_options(
    source: &Path,
    query: &str,
    scan_options: &ScanOptions,
) -> Result<Vec<RankedChunk>> {
    let terms = QueryTerms::parse(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let scan = scan_directory(source, scan_options)?;
    let extractor = TextExtractor;
    let mut chunks = Vec::new();

    for file in &scan.files {
        if !extractor.supports(file) {
            continue;
        }

        let document = extractor.extract(file)?;
        chunks.extend(split_document(&document));
    }

    Ok(rank_chunks(chunks, &terms, &DefaultScorer::default()))
}

pub fn rank_chunks<S>(
    chunks: impl IntoIterator<Item = Chunk>,
    terms: &QueryTerms,
    scorer: &S,
) -> Vec<RankedChunk>
where
    S: ChunkScorer,
{
    let mut ranked = Vec::new();

    for chunk in chunks {
        let Some(score_breakdown) = scorer.score(&chunk, terms) else {
            continue;
        };

        let preview = preview(&chunk.text);
        ranked.push(RankedChunk {
            path: chunk.path,
            kind: chunk.kind,
            title: chunk.title,
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            score: score_breakdown.total_score,
            token_estimate: chunk.token_estimate,
            text: chunk.text,
            preview,
            score_breakdown,
        });
    }

    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.start_line.cmp(&right.start_line))
    });

    ranked
}

fn count_matches(text: &str, terms: &[String]) -> usize {
    terms.iter().map(|term| text.matches(term).count()).sum()
}

fn density_bonus(text_matches: usize, term_count: usize, profile: ScoringProfile) -> usize {
    if text_matches < term_count || term_count == 0 {
        return 0;
    }

    (text_matches * profile.density_weight).min(8)
}

fn file_kind_bonus(path: &Path) -> usize {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("rs") => 5,
        Some("md" | "markdown") => 4,
        Some(
            "py" | "js" | "jsx" | "ts" | "tsx" | "java" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp"
            | "cs" | "go" | "rb" | "php" | "swift" | "kt" | "kts" | "scala" | "sh" | "bash" | "zsh"
            | "ps1" | "sql" | "lua" | "r" | "m" | "mm" | "dart" | "ex" | "exs" | "clj" | "cljs"
            | "fs" | "fsx" | "vb" | "gradle",
        ) => 4,
        Some("toml" | "json" | "yaml" | "yml" | "xml" | "html" | "htm") => 2,
        Some("csv" | "tsv") => 2,
        Some("pdf" | "docx") => 2,
        Some("txt" | "text" | "log" | "ini" | "cfg" | "conf" | "properties") => 1,
        _ => 0,
    }
}

fn chunk_kind_bonus(kind: ChunkKind) -> usize {
    match kind {
        ChunkKind::MarkdownSection => 3,
        ChunkKind::RustItem => 4,
        ChunkKind::CodeItem => 4,
        ChunkKind::TableRows => 2,
        ChunkKind::Paragraph => 0,
    }
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

    #[test]
    fn rank_chunks_orders_by_explainable_score() {
        let terms = QueryTerms::parse("ownership borrowing");
        let chunks = vec![
            Chunk {
                path: PathBuf::from("docs/notes.txt"),
                kind: ChunkKind::Paragraph,
                title: None,
                start_line: 1,
                end_line: 1,
                text: "ownership only".to_string(),
                token_estimate: 3,
            },
            Chunk {
                path: PathBuf::from("docs/ownership.md"),
                kind: ChunkKind::MarkdownSection,
                title: Some("Ownership".to_string()),
                start_line: 4,
                end_line: 4,
                text: "ownership borrowing ownership".to_string(),
                token_estimate: 8,
            },
        ];

        let ranked = rank_chunks(chunks, &terms, &DefaultScorer::default());

        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].path.ends_with("ownership.md"));
        assert!(ranked[0]
            .score_breakdown
            .reasons
            .iter()
            .any(|reason| reason.contains("text matches")));
    }
}
