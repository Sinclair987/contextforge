use std::path::PathBuf;

use serde::Serialize;

use crate::extract::{Document, DocumentKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkKind {
    Paragraph,
    MarkdownSection,
    RustItem,
    CodeItem,
    TableRows,
}

impl ChunkKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::MarkdownSection => "markdown section",
            Self::RustItem => "rust item",
            Self::CodeItem => "code item",
            Self::TableRows => "table rows",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub path: PathBuf,
    pub kind: ChunkKind,
    pub title: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub token_estimate: usize,
}

pub fn split_document(document: &Document) -> Vec<Chunk> {
    match document.kind {
        DocumentKind::Markdown => split_markdown_document(document),
        DocumentKind::Rust => split_rust_document(document),
        DocumentKind::Code => split_code_document(document),
        DocumentKind::Csv | DocumentKind::Tsv => split_delimited_document(document),
        DocumentKind::Text
        | DocumentKind::Toml
        | DocumentKind::Json
        | DocumentKind::Yaml
        | DocumentKind::Xml
        | DocumentKind::Html => split_paragraphs(document),
        DocumentKind::Pdf | DocumentKind::Docx => split_extracted_document(document),
    }
}

fn split_paragraphs(document: &Document) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current_lines = Vec::new();
    let mut start_line = 0;
    let mut previous_line = 0;

    for (index, line) in document.text.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() {
            push_chunk(
                &mut chunks,
                document,
                &mut current_lines,
                start_line,
                previous_line,
                ChunkKind::Paragraph,
                None,
            );
            start_line = 0;
            continue;
        }

        if current_lines.is_empty() {
            start_line = line_number;
        }

        current_lines.push(line.to_string());
        previous_line = line_number;
    }

    push_chunk(
        &mut chunks,
        document,
        &mut current_lines,
        start_line,
        previous_line,
        ChunkKind::Paragraph,
        None,
    );

    chunks
}

fn split_extracted_document(document: &Document) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current_lines = Vec::new();
    let mut start_line = 0;
    let mut previous_line = 0;

    for (index, line) in document.text.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let should_flush_current = !current_lines.is_empty()
            && (looks_like_extracted_heading(trimmed)
                || extracted_chunk_chars(&current_lines) + trimmed.chars().count() + 1
                    > MAX_EXTRACTED_CHUNK_CHARS);
        if should_flush_current {
            push_extracted_chunk(
                &mut chunks,
                document,
                &mut current_lines,
                start_line,
                previous_line,
            );
            start_line = line_number;
        } else if current_lines.is_empty() {
            start_line = line_number;
        }

        current_lines.push(trimmed.to_string());
        previous_line = line_number;

        if extracted_chunk_chars(&current_lines) >= TARGET_EXTRACTED_CHUNK_CHARS
            && ends_sentence(trimmed)
        {
            push_extracted_chunk(
                &mut chunks,
                document,
                &mut current_lines,
                start_line,
                previous_line,
            );
            start_line = 0;
            previous_line = 0;
        }
    }

    push_extracted_chunk(
        &mut chunks,
        document,
        &mut current_lines,
        start_line,
        previous_line,
    );

    chunks
}

fn split_markdown_document(document: &Document) -> Vec<Chunk> {
    if !document
        .text
        .lines()
        .any(|line| markdown_heading_title(line).is_some())
    {
        return split_paragraphs(document);
    }

    let mut chunks = Vec::new();
    let mut current_lines = Vec::new();
    let mut start_line = 0;
    let mut previous_line = 0;
    let mut title = None;

    for (index, line) in document.text.lines().enumerate() {
        let line_number = index + 1;
        if let Some(next_title) = markdown_heading_title(line) {
            push_chunk(
                &mut chunks,
                document,
                &mut current_lines,
                start_line,
                previous_line,
                ChunkKind::MarkdownSection,
                title.take(),
            );
            start_line = line_number;
            title = Some(next_title);
        } else if current_lines.is_empty() {
            start_line = line_number;
        }

        current_lines.push(line.to_string());
        previous_line = line_number;
    }

    push_chunk(
        &mut chunks,
        document,
        &mut current_lines,
        start_line,
        previous_line,
        ChunkKind::MarkdownSection,
        title,
    );

    chunks
        .into_iter()
        .filter(|chunk| !chunk.text.trim().is_empty())
        .collect()
}

fn split_rust_document(document: &Document) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current_lines = Vec::new();
    let mut start_line = 0;
    let mut previous_line = 0;
    let mut brace_depth = 0_usize;
    let mut title = None;
    let mut kind = ChunkKind::Paragraph;

    for (index, line) in document.text.lines().enumerate() {
        let line_number = index + 1;
        if brace_depth == 0 {
            if let Some(next_title) = rust_item_title(line) {
                push_chunk(
                    &mut chunks,
                    document,
                    &mut current_lines,
                    start_line,
                    previous_line,
                    kind,
                    title.take(),
                );
                start_line = line_number;
                title = Some(next_title);
                kind = ChunkKind::RustItem;
            } else if current_lines.is_empty() {
                start_line = line_number;
                kind = ChunkKind::Paragraph;
            }
        } else if current_lines.is_empty() {
            start_line = line_number;
        }

        current_lines.push(line.to_string());
        previous_line = line_number;
        brace_depth = update_brace_depth(brace_depth, line);
    }

    push_chunk(
        &mut chunks,
        document,
        &mut current_lines,
        start_line,
        previous_line,
        kind,
        title,
    );

    chunks
        .into_iter()
        .filter(|chunk| !chunk.text.trim().is_empty())
        .collect()
}

fn split_code_document(document: &Document) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current_lines = Vec::new();
    let mut start_line = 0;
    let mut previous_line = 0;
    let mut title = None;
    let mut kind = ChunkKind::Paragraph;

    for (index, line) in document.text.lines().enumerate() {
        let line_number = index + 1;
        if let Some(next_title) = code_item_title(line) {
            push_chunk(
                &mut chunks,
                document,
                &mut current_lines,
                start_line,
                previous_line,
                kind,
                title.take(),
            );
            start_line = line_number;
            title = Some(next_title);
            kind = ChunkKind::CodeItem;
        } else if current_lines.is_empty() && !line.trim().is_empty() {
            start_line = line_number;
            kind = ChunkKind::Paragraph;
        }

        if !line.trim().is_empty() || !current_lines.is_empty() {
            current_lines.push(line.to_string());
            previous_line = line_number;
        }
    }

    push_chunk(
        &mut chunks,
        document,
        &mut current_lines,
        start_line,
        previous_line,
        kind,
        title,
    );

    chunks
        .into_iter()
        .filter(|chunk| !chunk.text.trim().is_empty())
        .collect()
}

fn split_delimited_document(document: &Document) -> Vec<Chunk> {
    let lines = document
        .text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let Some((header_index, header)) = lines.first().copied() else {
        return Vec::new();
    };
    let rows = lines.iter().skip(1).copied().collect::<Vec<_>>();
    if rows.is_empty() {
        return vec![Chunk {
            path: document.path.clone(),
            kind: ChunkKind::TableRows,
            title: Some(header.to_string()),
            start_line: header_index + 1,
            end_line: header_index + 1,
            text: header.to_string(),
            token_estimate: estimate_tokens(header),
        }];
    }

    const ROWS_PER_CHUNK: usize = 25;
    let mut chunks = Vec::new();
    for group in rows.chunks(ROWS_PER_CHUNK) {
        let start_line = group
            .first()
            .map(|(index, _)| index + 1)
            .unwrap_or(header_index + 1);
        let end_line = group
            .last()
            .map(|(index, _)| index + 1)
            .unwrap_or(start_line);
        let mut text = String::new();
        text.push_str(header);
        for (_, row) in group {
            text.push('\n');
            text.push_str(row);
        }
        chunks.push(Chunk {
            path: document.path.clone(),
            kind: ChunkKind::TableRows,
            title: Some(header.to_string()),
            start_line,
            end_line,
            token_estimate: estimate_tokens(&text),
            text,
        });
    }

    chunks
}

fn push_chunk(
    chunks: &mut Vec<Chunk>,
    document: &Document,
    lines: &mut Vec<String>,
    start_line: usize,
    end_line: usize,
    kind: ChunkKind,
    title: Option<String>,
) {
    if lines.is_empty() {
        return;
    }

    let text = lines.join("\n");
    chunks.push(Chunk {
        path: document.path.clone(),
        kind,
        title,
        start_line,
        end_line,
        token_estimate: estimate_tokens(&text),
        text,
    });
    lines.clear();
}

const TARGET_EXTRACTED_CHUNK_CHARS: usize = 700;
const MAX_EXTRACTED_CHUNK_CHARS: usize = 1_200;

fn push_extracted_chunk(
    chunks: &mut Vec<Chunk>,
    document: &Document,
    lines: &mut Vec<String>,
    start_line: usize,
    end_line: usize,
) {
    let title = lines
        .first()
        .filter(|line| looks_like_extracted_heading(line))
        .cloned();
    push_chunk(
        chunks,
        document,
        lines,
        start_line,
        end_line,
        ChunkKind::Paragraph,
        title,
    );
}

fn extracted_chunk_chars(lines: &[String]) -> usize {
    lines
        .iter()
        .map(|line| line.chars().count() + 1)
        .sum::<usize>()
}

fn looks_like_extracted_heading(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 120 {
        return false;
    }

    starts_with_numbered_heading(trimmed)
        || starts_with_chinese_numbered_heading(trimmed)
        || markdown_heading_title(trimmed).is_some()
}

fn starts_with_numbered_heading(line: &str) -> bool {
    let mut chars = line.chars().peekable();
    let mut consumed_digit = false;

    while chars
        .peek()
        .is_some_and(|ch| ch.is_ascii_digit() || is_full_width_digit(*ch))
    {
        consumed_digit = true;
        chars.next();
    }

    if !consumed_digit {
        return false;
    }

    let mut consumed_marker = false;
    while chars
        .peek()
        .is_some_and(|ch| matches!(*ch, '.' | '．' | '、' | ')' | '）'))
    {
        consumed_marker = true;
        chars.next();
    }

    consumed_marker && chars.any(|ch| !ch.is_whitespace())
}

fn starts_with_chinese_numbered_heading(line: &str) -> bool {
    let mut chars = line.chars();
    let first = chars.next();
    let second = chars.next();

    if first.is_some_and(is_chinese_heading_number)
        && second.is_some_and(|ch| matches!(ch, '、' | '.' | '．' | ')' | '）'))
    {
        return true;
    }

    let lead = line.chars().take(8).collect::<String>();
    line.starts_with('第') && (lead.contains('章') || lead.contains('节') || lead.contains("部分"))
}

fn is_chinese_heading_number(ch: char) -> bool {
    matches!(
        ch,
        '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '零' | '〇'
    )
}

fn is_full_width_digit(ch: char) -> bool {
    matches!(ch as u32, 0xFF10..=0xFF19)
}

fn ends_sentence(line: &str) -> bool {
    line.chars()
        .rev()
        .find(|ch| !ch.is_whitespace())
        .is_some_and(|ch| matches!(ch, '.' | '。' | '!' | '！' | '?' | '？' | ';' | '；'))
}

pub(crate) fn estimate_tokens(text: &str) -> usize {
    let mut tokens = 0usize;
    let mut ascii_chars = 0usize;

    for character in text.chars() {
        if character.is_ascii() || character.is_whitespace() {
            ascii_chars += 1;
            continue;
        }

        tokens += ascii_chars.div_ceil(4);
        ascii_chars = 0;
        tokens += 1;
    }

    (tokens + ascii_chars.div_ceil(4)).max(1)
}

fn markdown_heading_title(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let marker_len = trimmed.chars().take_while(|ch| *ch == '#').count();
    if marker_len == 0 || marker_len > 6 {
        return None;
    }

    let title = trimmed.get(marker_len..)?.trim();
    if title.is_empty() {
        return None;
    }

    Some(title.trim_matches('#').trim().to_string())
}

fn rust_item_title(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("#[") {
        return None;
    }

    let item_prefixes = [
        "pub struct ",
        "struct ",
        "pub enum ",
        "enum ",
        "pub trait ",
        "trait ",
        "impl ",
        "pub fn ",
        "fn ",
        "pub mod ",
        "mod ",
    ];

    item_prefixes
        .iter()
        .find(|prefix| trimmed.starts_with(**prefix))
        .map(|_| {
            trimmed
                .trim_end_matches('{')
                .trim_end_matches(';')
                .trim()
                .to_string()
        })
}

fn code_item_title(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
    {
        return None;
    }

    let direct_prefixes = [
        "async def ",
        "def ",
        "class ",
        "function ",
        "export function ",
        "export default function ",
        "export class ",
        "interface ",
        "type ",
        "func ",
        "public class ",
        "private class ",
        "protected class ",
        "public interface ",
        "public enum ",
        "enum ",
        "record ",
        "CREATE TABLE ",
        "CREATE VIEW ",
        "CREATE FUNCTION ",
    ];
    if direct_prefixes
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
    {
        return Some(clean_code_title(trimmed));
    }

    let is_javascript_binding = trimmed.starts_with("const ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("export const ");
    let is_callable_binding = trimmed.contains("=>") || trimmed.contains("function");
    if is_javascript_binding && is_callable_binding {
        return Some(clean_code_title(trimmed));
    }

    if looks_like_c_family_function(trimmed) || looks_like_shell_function(trimmed) {
        return Some(clean_code_title(trimmed));
    }

    None
}

fn clean_code_title(line: &str) -> String {
    line.trim_end_matches('{')
        .trim_end_matches(':')
        .trim_end_matches(';')
        .trim()
        .to_string()
}

fn looks_like_c_family_function(line: &str) -> bool {
    if !line.ends_with('{') || !line.contains('(') || !line.contains(')') {
        return false;
    }

    let lower = line.to_ascii_lowercase();
    ![
        "if ", "for ", "while ", "switch ", "catch ", "else ", "do ", "try ",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn looks_like_shell_function(line: &str) -> bool {
    line.ends_with("{") && line.contains("()")
}

fn update_brace_depth(current: usize, line: &str) -> usize {
    let opens = line.chars().filter(|ch| *ch == '{').count();
    let closes = line.chars().filter(|ch| *ch == '}').count();
    current.saturating_add(opens).saturating_sub(closes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{Document, DocumentKind};

    #[test]
    fn split_document_creates_paragraph_chunks_with_line_numbers() {
        let document = Document {
            path: PathBuf::from("docs/rust.md"),
            kind: DocumentKind::Markdown,
            text: "# Ownership\nRust ownership matters.\n\nBorrowing prevents moves.\n".to_string(),
        };

        let chunks = split_document(&document);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 4);
        assert!(chunks[0].text.contains("Rust ownership matters."));
        assert_eq!(chunks[0].kind, ChunkKind::MarkdownSection);
    }

    #[test]
    fn split_document_groups_markdown_sections_by_heading() {
        let document = Document {
            path: PathBuf::from("docs/rust.md"),
            kind: DocumentKind::Markdown,
            text: "# Ownership\nBorrowing belongs with this heading.\n\n## Lifetimes\nLifetime notes stay separate.\n".to_string(),
        };

        let chunks = split_document(&document);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].kind, ChunkKind::MarkdownSection);
        assert_eq!(chunks[0].title.as_deref(), Some("Ownership"));
        assert!(chunks[0].text.contains("Borrowing belongs"));
        assert!(!chunks[0].text.contains("Lifetime notes"));
        assert_eq!(chunks[1].title.as_deref(), Some("Lifetimes"));
    }

    #[test]
    fn split_document_uses_paragraphs_for_markdown_without_headings() {
        let document = Document {
            path: PathBuf::from("docs/notes.md"),
            kind: DocumentKind::Markdown,
            text: "ownership borrowing paragraph one\n\nownership borrowing paragraph two\n"
                .to_string(),
        };

        let chunks = split_document(&document);

        assert_eq!(chunks.len(), 2);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.kind == ChunkKind::Paragraph));
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[1].start_line, 3);
    }

    #[test]
    fn split_document_creates_rust_item_chunks() {
        let document = Document {
            path: PathBuf::from("src/lib.rs"),
            kind: DocumentKind::Rust,
            text: "use std::fmt;\n\npub struct Runner {\n    name: String,\n}\n\nimpl Runner {\n    pub fn run(&self) {}\n}\n".to_string(),
        };

        let chunks = split_document(&document);

        assert!(chunks.iter().any(|chunk| chunk.kind == ChunkKind::RustItem
            && chunk.title.as_deref() == Some("pub struct Runner")));
        assert!(chunks.iter().any(|chunk| chunk.kind == ChunkKind::RustItem
            && chunk.title.as_deref() == Some("impl Runner")));
    }

    #[test]
    fn split_document_creates_code_item_chunks_for_common_languages() {
        let document = Document {
            path: PathBuf::from("scripts/build.py"),
            kind: DocumentKind::Code,
            text: "import os\n\nclass Builder:\n    pass\n\ndef plan_budget():\n    return 'ranking budget'\n"
                .to_string(),
        };

        let chunks = split_document(&document);

        assert!(chunks.iter().any(|chunk| chunk.kind == ChunkKind::CodeItem
            && chunk.title.as_deref() == Some("class Builder")));
        assert!(chunks.iter().any(|chunk| chunk.kind == ChunkKind::CodeItem
            && chunk.title.as_deref() == Some("def plan_budget()")));
    }

    #[test]
    fn split_document_groups_delimited_rows_with_header_context() {
        let document = Document {
            path: PathBuf::from("data/features.csv"),
            kind: DocumentKind::Csv,
            text: "name,description\nranking,budget scoring\nprivacy,redaction rules\n".to_string(),
        };

        let chunks = split_document(&document);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].kind, ChunkKind::TableRows);
        assert_eq!(chunks[0].title.as_deref(), Some("name,description"));
        assert!(chunks[0].text.contains("ranking,budget scoring"));
    }

    #[test]
    fn split_document_merges_short_extracted_pdf_lines_into_readable_sections() {
        let document = Document {
            path: PathBuf::from("docs/requirements.pdf"),
            kind: DocumentKind::Pdf,
            text: "1. Requirements\n\nshort ownership line one\n\nshort borrowing line two\n\nshort context line three\n\n2. Budget\n\nshort budget line one\n\nshort budget line two\n".to_string(),
        };

        let chunks = split_document(&document);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 7);
        assert!(chunks[0].text.contains("short borrowing line two"));
        assert_eq!(chunks[1].start_line, 9);
    }

    #[test]
    fn split_document_uses_extracted_document_chunking_for_docx() {
        let document = Document {
            path: PathBuf::from("docs/requirements.docx"),
            kind: DocumentKind::Docx,
            text: "Overview\n\nshort ownership line one\n\nshort borrowing line two\n\nshort context line three\n".to_string(),
        };

        let chunks = split_document(&document);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 7);
    }

    #[test]
    fn estimate_tokens_counts_cjk_characters_conservatively() {
        assert_eq!(estimate_tokens("中文上下文"), 5);
    }

    #[test]
    fn estimate_tokens_keeps_ascii_four_character_heuristic() {
        assert_eq!(estimate_tokens("abcdefgh"), 2);
    }
}
