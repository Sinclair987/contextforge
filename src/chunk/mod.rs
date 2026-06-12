use std::path::PathBuf;

use serde::Serialize;

use crate::extract::{Document, DocumentKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkKind {
    Paragraph,
    MarkdownSection,
    RustItem,
}

impl ChunkKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::MarkdownSection => "markdown section",
            Self::RustItem => "rust item",
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
        DocumentKind::Text
        | DocumentKind::Toml
        | DocumentKind::Json
        | DocumentKind::Pdf
        | DocumentKind::Docx => split_paragraphs(document),
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

fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() / 4).max(1)
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
}
