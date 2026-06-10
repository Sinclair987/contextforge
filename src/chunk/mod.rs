use std::path::PathBuf;

use crate::extract::Document;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub token_estimate: usize,
}

pub fn split_document(document: &Document) -> Vec<Chunk> {
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
    );

    chunks
}

fn push_chunk(
    chunks: &mut Vec<Chunk>,
    document: &Document,
    lines: &mut Vec<String>,
    start_line: usize,
    end_line: usize,
) {
    if lines.is_empty() {
        return;
    }

    let text = lines.join("\n");
    chunks.push(Chunk {
        path: document.path.clone(),
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

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 2);
        assert!(chunks[0].text.contains("Rust ownership matters."));
        assert_eq!(chunks[1].start_line, 4);
    }
}
