use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use quick_xml::{events::Event, Reader};
use zip::ZipArchive;

use crate::{
    scanner::{FileInfo, FileKind},
    ContextForgeError, Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Markdown,
    Rust,
    Text,
    Toml,
    Json,
    Pdf,
    Docx,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub path: PathBuf,
    pub kind: DocumentKind,
    pub text: String,
}

pub trait Extractor {
    fn supports(&self, file: &FileInfo) -> bool;
    fn extract(&self, file: &FileInfo) -> Result<Document>;
}

#[derive(Debug, Default)]
pub struct TextExtractor;

impl Extractor for TextExtractor {
    fn supports(&self, file: &FileInfo) -> bool {
        document_kind(file.kind).is_some()
    }

    fn extract(&self, file: &FileInfo) -> Result<Document> {
        let Some(kind) = document_kind(file.kind) else {
            return Err(ContextForgeError::UnsupportedFileKind {
                path: file.path.clone(),
            });
        };

        let text = match kind {
            DocumentKind::Markdown
            | DocumentKind::Rust
            | DocumentKind::Text
            | DocumentKind::Toml
            | DocumentKind::Json => read_utf8_text(&file.path)?,
            DocumentKind::Pdf => extract_pdf_text(&file.path)?,
            DocumentKind::Docx => extract_docx_text(&file.path)?,
        };

        Ok(Document {
            path: file.path.clone(),
            kind,
            text,
        })
    }
}

fn read_utf8_text(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| ContextForgeError::ReadFile {
        path: path.to_path_buf(),
        source,
    })
}

fn extract_pdf_text(path: &Path) -> Result<String> {
    let text = pdf_extract::extract_text(path).map_err(|source| ContextForgeError::ExtractPdf {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(normalize_extracted_document_text(&text))
}

fn extract_docx_text(path: &Path) -> Result<String> {
    let file = fs::File::open(path).map_err(|source| ContextForgeError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive =
        ZipArchive::new(file).map_err(|source| ContextForgeError::OpenDocxArchive {
            path: path.to_path_buf(),
            source,
        })?;
    let mut document_xml = archive.by_name("word/document.xml").map_err(|source| {
        ContextForgeError::ReadDocxEntry {
            path: path.to_path_buf(),
            source,
        }
    })?;
    let mut xml = String::new();
    document_xml
        .read_to_string(&mut xml)
        .map_err(|source| ContextForgeError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;

    extract_docx_xml_text(path, &xml).map(|text| normalize_extracted_document_text(&text))
}

fn extract_docx_xml_text(path: &Path, xml: &str) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut text = String::new();
    let mut in_text_node = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if event.name().as_ref() == b"w:t" => {
                in_text_node = true;
            }
            Ok(Event::End(event)) if event.name().as_ref() == b"w:t" => {
                in_text_node = false;
            }
            Ok(Event::End(event)) if event.name().as_ref() == b"w:p" => {
                if !text.ends_with('\n') {
                    text.push('\n');
                }
            }
            Ok(Event::Empty(event)) if event.name().as_ref() == b"w:tab" => {
                text.push('\t');
            }
            Ok(Event::Empty(event)) if event.name().as_ref() == b"w:br" => {
                text.push('\n');
            }
            Ok(Event::Text(event)) if in_text_node => {
                let value =
                    event
                        .xml10_content()
                        .map_err(|source| ContextForgeError::DecodeDocxXml {
                            path: path.to_path_buf(),
                            source,
                        })?;
                text.push_str(&value);
            }
            Ok(Event::Eof) => break,
            Err(source) => {
                return Err(ContextForgeError::ParseDocxXml {
                    path: path.to_path_buf(),
                    source,
                });
            }
            _ => {}
        }
    }

    Ok(text)
}

fn normalize_extracted_document_text(text: &str) -> String {
    let canonical_newlines = text.replace("\r\n", "\n").replace('\r', "\n");
    let cleaned = canonical_newlines
        .chars()
        .map(|character| match character {
            '\n' => '\n',
            '\t' => ' ',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect::<String>();

    let mut lines = Vec::new();
    let mut previous_blank = false;
    for line in cleaned.lines() {
        let collapsed = collapse_line_whitespace(line);
        if collapsed.is_empty() {
            if !previous_blank {
                lines.push(String::new());
            }
            previous_blank = true;
            continue;
        }
        previous_blank = false;
        lines.push(collapsed);
    }

    lines.join("\n")
}

fn collapse_line_whitespace(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn document_kind(kind: FileKind) -> Option<DocumentKind> {
    match kind {
        FileKind::Markdown => Some(DocumentKind::Markdown),
        FileKind::Rust => Some(DocumentKind::Rust),
        FileKind::Text => Some(DocumentKind::Text),
        FileKind::Toml => Some(DocumentKind::Toml),
        FileKind::Json => Some(DocumentKind::Json),
        FileKind::Pdf => Some(DocumentKind::Pdf),
        FileKind::Docx => Some(DocumentKind::Docx),
        FileKind::Other => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::FileInfo;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn text_extractor_reads_supported_file_text() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join("notes.md");
        fs::write(&path, "# Ownership\nBorrowing notes\n").expect("markdown file");
        let file = FileInfo {
            path: path.clone(),
            size_bytes: 28,
            kind: FileKind::Markdown,
        };

        let extractor = TextExtractor;
        let document = extractor.extract(&file).expect("document");

        assert_eq!(document.kind, DocumentKind::Markdown);
        assert_eq!(document.path, path);
        assert!(document.text.contains("Borrowing notes"));
    }

    #[test]
    fn text_extractor_rejects_unsupported_file_kind() {
        let file = FileInfo {
            path: PathBuf::from("image.svg"),
            size_bytes: 0,
            kind: FileKind::Other,
        };

        let extractor = TextExtractor;

        assert!(!extractor.supports(&file));
    }

    #[test]
    fn document_text_normalization_removes_control_characters() {
        let normalized = normalize_extracted_document_text("Rust\u{1}PDF\r\nNext\tword\n\n\nTail");

        assert_eq!(normalized, "Rust PDF\nNext word\n\nTail");
    }
}
