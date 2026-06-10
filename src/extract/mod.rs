use std::{fs, path::PathBuf};

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

        let text =
            fs::read_to_string(&file.path).map_err(|source| ContextForgeError::ReadFile {
                path: file.path.clone(),
                source,
            })?;

        Ok(Document {
            path: file.path.clone(),
            kind,
            text,
        })
    }
}

fn document_kind(kind: FileKind) -> Option<DocumentKind> {
    match kind {
        FileKind::Markdown => Some(DocumentKind::Markdown),
        FileKind::Rust => Some(DocumentKind::Rust),
        FileKind::Text => Some(DocumentKind::Text),
        FileKind::Toml => Some(DocumentKind::Toml),
        FileKind::Json => Some(DocumentKind::Json),
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
}
