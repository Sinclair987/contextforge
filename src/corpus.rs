use std::path::PathBuf;

use serde::Serialize;

use crate::{
    audit::{audit_text, PrivacyFinding},
    chunk::{split_document, Chunk},
    extract::{Extractor, TextExtractor},
    scanner::{scan_directory, ScanOptions, ScanSummary},
    Result,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtractionIssue {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Corpus {
    pub scan: ScanSummary,
    pub chunks: Vec<Chunk>,
    pub privacy_findings: Vec<PrivacyFinding>,
    pub extraction_issues: Vec<ExtractionIssue>,
}

pub fn load_corpus(source: &std::path::Path, options: &ScanOptions) -> Result<Corpus> {
    let scan = scan_directory(source, options)?;
    let extractor = TextExtractor;
    let mut chunks = Vec::new();
    let mut privacy_findings = Vec::new();
    let mut extraction_issues = Vec::new();

    for file in &scan.files {
        if !extractor.supports(file) {
            continue;
        }

        match extractor.extract(file) {
            Ok(document) => {
                privacy_findings.extend(audit_text(&document.path, &document.text));
                chunks.extend(split_document(&document));
            }
            Err(error) => extraction_issues.push(ExtractionIssue {
                path: file.path.clone(),
                message: error.to_string(),
            }),
        }
    }

    privacy_findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.kind.label().cmp(right.kind.label()))
    });

    Ok(Corpus {
        scan,
        chunks,
        privacy_findings,
        extraction_issues,
    })
}
