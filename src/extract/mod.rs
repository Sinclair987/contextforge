use std::{
    any::Any,
    cell::Cell,
    fs,
    io::Read,
    panic::{self, UnwindSafe},
    path::{Path, PathBuf},
    sync::Once,
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
    Code,
    Text,
    Toml,
    Json,
    Yaml,
    Csv,
    Tsv,
    Xml,
    Html,
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
            | DocumentKind::Code
            | DocumentKind::Text
            | DocumentKind::Toml
            | DocumentKind::Json
            | DocumentKind::Yaml
            | DocumentKind::Csv
            | DocumentKind::Tsv => read_utf8_text(&file.path)?,
            DocumentKind::Xml | DocumentKind::Html => {
                let text = read_utf8_text(&file.path)?;
                extract_markup_text(&text)
            }
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
    let text = run_pdf_extractor(path, || pdf_extract::extract_text(path))?;
    Ok(normalize_extracted_document_text(&text))
}

thread_local! {
    static PDF_EXTRACTION_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

static INSTALL_PDF_PANIC_HOOK: Once = Once::new();

fn run_pdf_extractor<F>(path: &Path, extract: F) -> Result<String>
where
    F: FnOnce() -> std::result::Result<String, pdf_extract::OutputError> + UnwindSafe,
{
    install_pdf_panic_hook();
    let previous_state = PDF_EXTRACTION_ACTIVE.with(|active| active.replace(true));
    let extracted = panic::catch_unwind(extract);
    PDF_EXTRACTION_ACTIVE.with(|active| active.set(previous_state));

    match extracted {
        Ok(result) => result.map_err(|source| ContextForgeError::ExtractPdf {
            path: path.to_path_buf(),
            source,
        }),
        Err(payload) => Err(ContextForgeError::ExtractPdfPanic {
            path: path.to_path_buf(),
            message: panic_payload_message(payload.as_ref()),
        }),
    }
}

fn install_pdf_panic_hook() {
    INSTALL_PDF_PANIC_HOOK.call_once(|| {
        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !PDF_EXTRACTION_ACTIVE.with(Cell::get) {
                previous_hook(info);
            }
        }));
    });
}

fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "unknown PDF extractor failure".to_string()
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
        .map(|character| {
            let character = crate::normalize::normalize_width_and_radicals(character);
            match character {
                '\n' => '\n',
                '\t' => ' ',
                character if character.is_control() => ' ',
                character => character,
            }
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

    let first_content = lines
        .iter()
        .position(|line| !line.is_empty())
        .unwrap_or(lines.len());
    let last_content = lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map(|index| index + 1)
        .unwrap_or(first_content);

    lines[first_content..last_content].join("\n")
}

fn collapse_line_whitespace(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_markup_text(text: &str) -> String {
    let mut output = String::new();
    let mut chars = text.chars().peekable();
    let mut skip_tag: Option<String> = None;

    while let Some(character) = chars.next() {
        if character != '<' {
            if skip_tag.is_none() {
                output.push(character);
            }
            continue;
        }

        let tag = read_markup_tag(&mut chars);
        let tag_name = markup_tag_name(&tag);
        if let Some(skipped) = skip_tag.as_deref() {
            if tag.starts_with('/') && tag_name == skipped {
                skip_tag = None;
                push_markup_break(&mut output);
            }
            continue;
        }

        if matches!(tag_name.as_str(), "script" | "style") && !tag.starts_with('/') {
            skip_tag = Some(tag_name);
            continue;
        }

        if is_block_markup_tag(&tag_name) || tag.starts_with('/') {
            push_markup_break(&mut output);
        } else {
            push_markup_space(&mut output);
        }
    }

    normalize_extracted_document_text(&decode_markup_entities(&output))
}

fn push_markup_break(output: &mut String) {
    if !output.ends_with('\n') {
        output.push('\n');
    }
}

fn push_markup_space(output: &mut String) {
    if !output.chars().last().is_some_and(char::is_whitespace) {
        output.push(' ');
    }
}

fn read_markup_tag(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut tag = String::new();
    for character in chars.by_ref() {
        if character == '>' {
            break;
        }
        tag.push(character);
    }
    tag.trim().to_ascii_lowercase()
}

fn markup_tag_name(tag: &str) -> String {
    tag.trim_start_matches('/')
        .split(|character: char| character.is_whitespace() || character == '/' || character == '>')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn is_block_markup_tag(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "article"
            | "aside"
            | "blockquote"
            | "body"
            | "br"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hr"
            | "html"
            | "li"
            | "main"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "td"
            | "th"
            | "tr"
            | "ul"
    )
}

fn decode_markup_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

fn document_kind(kind: FileKind) -> Option<DocumentKind> {
    match kind {
        FileKind::Markdown => Some(DocumentKind::Markdown),
        FileKind::Rust => Some(DocumentKind::Rust),
        FileKind::Code => Some(DocumentKind::Code),
        FileKind::Text => Some(DocumentKind::Text),
        FileKind::Toml => Some(DocumentKind::Toml),
        FileKind::Json => Some(DocumentKind::Json),
        FileKind::Yaml => Some(DocumentKind::Yaml),
        FileKind::Csv => Some(DocumentKind::Csv),
        FileKind::Tsv => Some(DocumentKind::Tsv),
        FileKind::Xml => Some(DocumentKind::Xml),
        FileKind::Html => Some(DocumentKind::Html),
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
    fn text_extractor_strips_html_markup_to_readable_text() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join("index.html");
        fs::write(
            &path,
            "<html><body><h1>Ownership</h1><p>Borrowing &amp; lifetimes</p></body></html>",
        )
        .expect("html file");
        let file = FileInfo {
            path: path.clone(),
            size_bytes: 80,
            kind: FileKind::Html,
        };

        let document = TextExtractor.extract(&file).expect("document");

        assert_eq!(document.kind, DocumentKind::Html);
        assert_eq!(document.text, "Ownership\nBorrowing & lifetimes");
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
    fn pdf_extraction_converts_dependency_panics_to_errors() {
        let error = run_pdf_extractor(Path::new("gbk.pdf"), || {
            panic!("unsupported encoding GBK-EUC-H")
        })
        .expect_err("PDF extraction error");

        assert_eq!(
            error.to_string(),
            "PDF extractor could not process `gbk.pdf`: unsupported encoding GBK-EUC-H"
        );
    }

    #[test]
    fn document_text_normalization_removes_control_characters() {
        let normalized = normalize_extracted_document_text("Rust\u{1}PDF\r\nNext\tword\n\n\nTail");

        assert_eq!(normalized, "Rust PDF\nNext word\n\nTail");
    }

    #[test]
    fn document_text_normalization_canonicalizes_pdf_compatibility_characters() {
        let normalized = normalize_extracted_document_text(
            "\u{FF32}\u{FF55}\u{FF53}\u{FF54}\n\u{4F5C}\u{4E1A}\u{2F6C}\u{6807} \u{2F50}\u{4F8B}",
        );

        assert_eq!(
            normalized,
            "Rust\n\u{4F5C}\u{4E1A}\u{76EE}\u{6807} \u{6BD4}\u{4F8B}"
        );
    }

    #[test]
    fn document_text_normalization_canonicalizes_more_pdf_radicals() {
        let normalized = normalize_extracted_document_text(
            "\u{2FCE}\u{52B1} \u{6EE1}\u{2F9C} \u{2EDA}\u{2FAF} \u{2F79}\u{7EDC} \u{2EC5}\u{89C6}",
        );

        assert_eq!(
            normalized,
            "\u{9F13}\u{52B1} \u{6EE1}\u{8DB3} \u{9875}\u{9762} \u{7F51}\u{7EDC} \u{89C1}\u{89C6}"
        );
    }
}
