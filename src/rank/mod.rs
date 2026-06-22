use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    chunk::{Chunk, ChunkKind},
    corpus::load_corpus,
    scanner::ScanOptions,
    Result,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTerms {
    terms: Vec<String>,
}

impl QueryTerms {
    pub fn parse(query: &str) -> Self {
        let mut terms = Vec::new();

        for term in tokenize(query) {
            if terms.contains(&term) {
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
    pub bm25_weight: usize,
    pub text_match_weight: usize,
    pub term_coverage_weight: usize,
    pub full_coverage_weight: usize,
    pub path_match_weight: usize,
    pub title_match_weight: usize,
    pub file_name_match_weight: usize,
    pub density_weight: usize,
}

impl Default for ScoringProfile {
    fn default() -> Self {
        Self {
            bm25_weight: 10,
            text_match_weight: 3,
            term_coverage_weight: 6,
            full_coverage_weight: 18,
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

#[derive(Debug, Clone, PartialEq)]
pub struct CorpusScorer {
    profile: ScoringProfile,
    document_count: usize,
    average_document_len: f64,
    document_frequencies: BTreeMap<String, usize>,
}

impl CorpusScorer {
    pub fn from_chunks(chunks: &[Chunk], profile: ScoringProfile) -> Self {
        let mut document_frequencies = BTreeMap::<String, usize>::new();
        let mut total_terms = 0usize;

        for chunk in chunks {
            let tokens = tokenize(&chunk.text);
            total_terms += tokens.len();

            for term in unique_terms(tokens) {
                *document_frequencies.entry(term).or_default() += 1;
            }
        }

        let document_count = chunks.len();
        let average_document_len = if document_count == 0 {
            1.0
        } else {
            (total_terms as f64 / document_count as f64).max(1.0)
        };

        Self {
            profile,
            document_count,
            average_document_len,
            document_frequencies,
        }
    }

    fn idf(&self, term: &str) -> f64 {
        let document_count = self.document_count.max(1) as f64;
        let document_frequency = self
            .document_frequencies
            .get(term)
            .copied()
            .unwrap_or_default() as f64;

        (1.0 + ((document_count - document_frequency + 0.5) / (document_frequency + 0.5))).ln()
    }

    fn bm25_score(
        &self,
        frequencies: &BTreeMap<String, usize>,
        document_len: usize,
        terms: &QueryTerms,
    ) -> f64 {
        const K1: f64 = 1.2;
        const B: f64 = 0.75;

        let document_len = document_len.max(1) as f64;
        let length_normalizer = 1.0 - B + B * (document_len / self.average_document_len);

        terms
            .as_slice()
            .iter()
            .filter_map(|term| {
                let term_frequency = frequencies.get(term).copied().unwrap_or_default() as f64;
                if term_frequency == 0.0 {
                    return None;
                }

                let numerator = term_frequency * (K1 + 1.0);
                let denominator = term_frequency + K1 * length_normalizer;
                Some(self.idf(term) * (numerator / denominator))
            })
            .sum()
    }
}

impl ChunkScorer for CorpusScorer {
    fn score(&self, chunk: &Chunk, terms: &QueryTerms) -> Option<ScoreBreakdown> {
        if terms.is_empty() {
            return None;
        }

        let text_tokens = tokenize(&chunk.text);
        let text_frequencies = term_frequencies(&text_tokens);
        let path = normalize_search_text(&chunk.path.to_string_lossy());
        let title = chunk
            .title
            .as_deref()
            .map(normalize_search_text)
            .unwrap_or_default();
        let file_name = chunk
            .path
            .file_name()
            .map(|name| normalize_search_text(&name.to_string_lossy()))
            .unwrap_or_default();

        let text_matches = terms
            .as_slice()
            .iter()
            .map(|term| text_frequencies.get(term).copied().unwrap_or_default())
            .sum::<usize>();
        let path_matches = count_matches(&path, terms.as_slice());
        let title_matches = count_matches(&title, terms.as_slice());
        let file_name_matches = count_matches(&file_name, terms.as_slice());
        let total_matches = text_matches + path_matches + title_matches + file_name_matches;

        if total_matches == 0 {
            return None;
        }

        let text_matched_terms = terms
            .as_slice()
            .iter()
            .filter(|term| text_frequencies.contains_key(term.as_str()))
            .count();
        let bm25_score = self.bm25_score(&text_frequencies, text_tokens.len(), terms);
        let lexical_score = (bm25_score * self.profile.bm25_weight as f64).round() as usize;
        let text_match_score = text_matched_terms * self.profile.text_match_weight;
        let covered_terms = terms
            .as_slice()
            .iter()
            .filter(|term| {
                text_frequencies.contains_key(term.as_str())
                    || path.contains(term.as_str())
                    || title.contains(term.as_str())
                    || file_name.contains(term.as_str())
            })
            .count();
        let term_coverage_score = covered_terms * self.profile.term_coverage_weight;
        let full_coverage_score =
            if terms.as_slice().len() > 1 && covered_terms == terms.as_slice().len() {
                self.profile.full_coverage_weight
            } else {
                0
            };
        let path_match_score = path_matches * self.profile.path_match_weight;
        let title_match_score = title_matches * self.profile.title_match_weight;
        let file_name_match_score = file_name_matches * self.profile.file_name_match_weight;
        let chunk_kind_score = chunk_kind_bonus(chunk.kind);
        let file_kind_score = file_kind_bonus(&chunk.path) + chunk_kind_score;
        let density_score = density_bonus(text_matches, terms.as_slice().len(), self.profile);
        let total_score = lexical_score
            + text_match_score
            + term_coverage_score
            + full_coverage_score
            + path_match_score
            + title_match_score
            + file_name_match_score
            + file_kind_score
            + density_score;

        let mut reasons = Vec::new();
        if lexical_score > 0 {
            reasons.push(format!("BM25 lexical score: {lexical_score}"));
        }
        if text_matches > 0 {
            reasons.push(format!(
                "exact text matches: {text_matches}, unique terms {text_matched_terms}/{} x {}",
                terms.as_slice().len(),
                self.profile.text_match_weight
            ));
        }
        if term_coverage_score > 0 {
            reasons.push(format!(
                "term coverage: {covered_terms}/{} x {}",
                terms.as_slice().len(),
                self.profile.term_coverage_weight
            ));
        }
        if full_coverage_score > 0 {
            reasons.push(format!("full query coverage bonus: {full_coverage_score}"));
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
            lexical_score,
            text_match_score,
            term_coverage_score,
            full_coverage_score,
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
    pub lexical_score: usize,
    pub text_match_score: usize,
    pub term_coverage_score: usize,
    pub full_coverage_score: usize,
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

    let corpus = load_corpus(source, scan_options)?;
    Ok(rank_chunks(corpus.chunks, &terms))
}

pub fn rank_chunks(
    chunks: impl IntoIterator<Item = Chunk>,
    terms: &QueryTerms,
) -> Vec<RankedChunk> {
    let chunks = chunks.into_iter().collect::<Vec<_>>();
    let scorer = CorpusScorer::from_chunks(&chunks, ScoringProfile::default());
    rank_chunks_with_scorer(chunks, terms, &scorer)
}

pub fn rank_chunks_with_scorer<S>(
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenClass {
    Cjk,
    Word,
}

fn tokenize(text: &str) -> Vec<String> {
    let normalized = normalize_search_text(text);
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut current_class = None;

    for ch in normalized.chars() {
        let Some(class) = token_class(ch) else {
            flush_token(&mut tokens, &mut current, current_class);
            current_class = None;
            continue;
        };

        if current_class.is_some_and(|active| active != class) {
            flush_token(&mut tokens, &mut current, current_class);
        }

        current.push(ch);
        current_class = Some(class);
    }

    flush_token(&mut tokens, &mut current, current_class);
    tokens
}

fn flush_token(tokens: &mut Vec<String>, current: &mut String, class: Option<TokenClass>) {
    if current.is_empty() {
        return;
    }

    tokens.push(std::mem::take(current));
    if class == Some(TokenClass::Cjk) {
        let token = tokens.last().expect("token was just pushed").clone();
        add_cjk_ngrams(tokens, &token);
    }
}

fn add_cjk_ngrams(tokens: &mut Vec<String>, token: &str) {
    let chars = token.chars().collect::<Vec<_>>();
    for size in [2, 3, 4] {
        if chars.len() < size {
            continue;
        }

        for window in chars.windows(size) {
            tokens.push(window.iter().collect());
        }
    }
}

fn token_class(ch: char) -> Option<TokenClass> {
    if is_cjk(ch) {
        return Some(TokenClass::Cjk);
    }

    (ch.is_alphanumeric() || ch == '_').then_some(TokenClass::Word)
}

fn normalize_search_text(text: &str) -> String {
    let mut normalized = String::new();
    for ch in text.chars() {
        for lower in normalize_char(ch).to_lowercase() {
            normalized.push(lower);
        }
    }
    normalized
}

fn normalize_char(ch: char) -> char {
    crate::normalize::normalize_search_char(ch)
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0x2CEB0..=0x2EBEF
            | 0x30000..=0x3134F
    )
}

fn unique_terms(tokens: Vec<String>) -> BTreeSet<String> {
    tokens.into_iter().collect()
}

fn term_frequencies(tokens: &[String]) -> BTreeMap<String, usize> {
    let mut frequencies = BTreeMap::new();
    for token in tokens {
        *frequencies.entry(token.clone()).or_default() += 1;
    }
    frequencies
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

        let ranked = rank_chunks(chunks, &terms);

        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].path.ends_with("ownership.md"));
        assert!(ranked[0]
            .score_breakdown
            .reasons
            .iter()
            .any(|reason| reason.contains("BM25")));
    }

    #[test]
    fn rank_chunks_prefers_focused_multi_term_match_over_repetition() {
        let terms = QueryTerms::parse("neural budget");
        let repeated_common_term = "budget ".repeat(40);
        let chunks = vec![
            Chunk {
                path: PathBuf::from("docs/common.txt"),
                kind: ChunkKind::Paragraph,
                title: None,
                start_line: 1,
                end_line: 1,
                text: repeated_common_term,
                token_estimate: 40,
            },
            Chunk {
                path: PathBuf::from("docs/focused.md"),
                kind: ChunkKind::MarkdownSection,
                title: Some("Neural budget".to_string()),
                start_line: 1,
                end_line: 2,
                text: "neural budget plan".to_string(),
                token_estimate: 4,
            },
        ];

        let ranked = rank_chunks(chunks, &terms);

        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].path.ends_with("focused.md"));
        assert!(ranked[0]
            .score_breakdown
            .reasons
            .iter()
            .any(|reason| reason.contains("term coverage")));
    }

    #[test]
    fn rank_chunks_rewards_complete_query_coverage() {
        let terms = QueryTerms::parse("ranking budget");
        let chunks = vec![
            Chunk {
                path: PathBuf::from("src/budget.rs"),
                kind: ChunkKind::RustItem,
                title: Some("BudgetBudgetBudget".to_string()),
                start_line: 1,
                end_line: 3,
                text: "budget ".repeat(20),
                token_estimate: 20,
            },
            Chunk {
                path: PathBuf::from("docs/relevance.txt"),
                kind: ChunkKind::Paragraph,
                title: None,
                start_line: 1,
                end_line: 1,
                text: "ranking budget overview".to_string(),
                token_estimate: 4,
            },
        ];

        let ranked = rank_chunks(chunks, &terms);

        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].path.ends_with("relevance.txt"));
        assert!(ranked[0]
            .score_breakdown
            .reasons
            .iter()
            .any(|reason| reason.contains("full query coverage")));
    }

    #[test]
    fn rank_chunks_matches_related_chinese_goal_with_extra_character() {
        let terms = QueryTerms::parse("\u{671F}\u{672B}\u{5927}\u{4F5C}\u{4E1A}\u{8981}\u{6C42}");
        let chunks = vec![Chunk {
            path: PathBuf::from("\u{671F}\u{672B}\u{4F5C}\u{4E1A}\u{8981}\u{6C42}.pdf"),
            kind: ChunkKind::Paragraph,
            title: None,
            start_line: 1,
            end_line: 2,
            text: "\u{671F}\u{672B}\u{4F5C}\u{4E1A}\u{8981}\u{6C42}\n\u{63A8}\u{8350}\u{89C4}\u{6A21}\u{FF1A}1500\u{301C}3000 \u{884C}\u{6709}\u{6548} Rust \u{4EE3}\u{7801}".to_string(),
            token_estimate: 20,
        }];

        let ranked = rank_chunks(chunks, &terms);

        assert_eq!(ranked.len(), 1);
        assert!(ranked[0]
            .path
            .ends_with("\u{671F}\u{672B}\u{4F5C}\u{4E1A}\u{8981}\u{6C42}.pdf"));
    }

    #[test]
    fn rank_chunks_normalizes_cjk_compatibility_radicals() {
        let terms = QueryTerms::parse("\u{5927}\u{4F5C}\u{4E1A}");
        let chunks = vec![Chunk {
            path: PathBuf::from("requirements.pdf"),
            kind: ChunkKind::Paragraph,
            title: None,
            start_line: 1,
            end_line: 1,
            text: "\u{672C}\u{8BFE}\u{7A0B}\u{671F}\u{672B}\u{2F24}\u{4F5C}\u{4E1A}\u{65E8}\u{5728}\u{5E2E}\u{52A9}\u{5B66}\u{751F}\u{7EFC}\u{5408}\u{8FD0}\u{7528} Rust \u{8BED}\u{8A00}\u{77E5}\u{8BC6}".to_string(),
            token_estimate: 20,
        }];

        let ranked = rank_chunks(chunks, &terms);

        assert_eq!(ranked.len(), 1);
        assert!(ranked[0].preview.contains("\u{671F}\u{672B}"));
    }

    #[test]
    fn rank_chunks_normalizes_full_width_ascii_and_additional_kangxi_radicals() {
        let terms = QueryTerms::parse("Rust \u{6BD4}\u{4F8B}");
        let chunks = vec![Chunk {
            path: PathBuf::from("requirements.pdf"),
            kind: ChunkKind::Paragraph,
            title: None,
            start_line: 1,
            end_line: 1,
            text: "\u{FF32}\u{FF55}\u{FF53}\u{FF54} \u{2F50}\u{4F8B} requirement".to_string(),
            token_estimate: 10,
        }];

        let ranked = rank_chunks(chunks, &terms);

        assert_eq!(ranked.len(), 1);
        assert!(ranked[0]
            .score_breakdown
            .reasons
            .iter()
            .any(|reason| reason.contains("full query coverage")));
    }
}
