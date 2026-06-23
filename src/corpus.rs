use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    thread,
};

use serde::Serialize;

use crate::{
    audit::{audit_text, PrivacyFinding},
    chunk::{split_document, Chunk},
    extract::{Extractor, TextExtractor},
    scanner::{scan_directory, FileInfo, FileKind, ScanOptions, ScanSummary},
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

const MAX_PDF_WORKERS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtractedFile {
    pub file: FileInfo,
    pub chunks: Vec<Chunk>,
    pub privacy_findings: Vec<PrivacyFinding>,
    pub issue: Option<ExtractionIssue>,
}

pub fn load_corpus(source: &std::path::Path, options: &ScanOptions) -> Result<Corpus> {
    let scan = scan_directory(source, options)?;
    let mut chunks = Vec::new();
    let mut privacy_findings = Vec::new();
    let mut extraction_issues = Vec::new();

    for extracted in extract_files(&scan.files, options.pdf_timeout_seconds) {
        chunks.extend(extracted.chunks);
        privacy_findings.extend(extracted.privacy_findings);
        if let Some(issue) = extracted.issue {
            extraction_issues.push(issue);
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

pub(crate) fn extract_files(files: &[FileInfo], pdf_timeout_seconds: u64) -> Vec<ExtractedFile> {
    let extractor = TextExtractor::new(pdf_timeout_seconds);
    let mut outcomes = Vec::new();
    let mut pdf_files = Vec::new();
    for (index, file) in files.iter().enumerate() {
        if file.kind == FileKind::Pdf {
            pdf_files.push((index, file));
        } else {
            outcomes.push((index, extract_file(&extractor, file)));
        }
    }

    outcomes.extend(bounded_parallel_map(
        &pdf_files,
        pdf_worker_limit(),
        |(index, file)| (*index, extract_file(&extractor, file)),
    ));
    outcomes.sort_by_key(|(index, _)| *index);
    outcomes.into_iter().map(|(_, outcome)| outcome).collect()
}

fn pdf_worker_limit() -> usize {
    thread::available_parallelism()
        .map_or(1, usize::from)
        .min(MAX_PDF_WORKERS)
}

fn extract_file(extractor: &TextExtractor, file: &FileInfo) -> ExtractedFile {
    if !extractor.supports(file) {
        return ExtractedFile {
            file: file.clone(),
            chunks: Vec::new(),
            privacy_findings: Vec::new(),
            issue: None,
        };
    }

    match extractor.extract(file) {
        Ok(document) => ExtractedFile {
            file: file.clone(),
            chunks: split_document(&document),
            privacy_findings: audit_text(&document.path, &document.text),
            issue: None,
        },
        Err(error) => ExtractedFile {
            file: file.clone(),
            chunks: Vec::new(),
            privacy_findings: Vec::new(),
            issue: Some(ExtractionIssue {
                path: file.path.clone(),
                message: error.to_string(),
            }),
        },
    }
}

fn bounded_parallel_map<T, U, F>(items: &[T], worker_limit: usize, operation: F) -> Vec<U>
where
    T: Sync,
    U: Send,
    F: Fn(&T) -> U + Sync,
{
    if items.is_empty() {
        return Vec::new();
    }

    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(items.len()));
    let worker_count = worker_limit.max(1).min(items.len());

    thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else {
                    break;
                };
                let output = operation(item);
                results
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push((index, output));
            });
        }
    });

    let mut results = results
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, output)| output).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        thread,
        time::Duration,
    };

    #[test]
    fn bounded_parallel_map_preserves_order_and_worker_limit() {
        let active = AtomicUsize::new(0);
        let maximum_active = AtomicUsize::new(0);
        let values = [1, 2, 3, 4, 5, 6];

        let doubled = bounded_parallel_map(&values, 3, |value| {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum_active.fetch_max(current, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(20));
            active.fetch_sub(1, Ordering::SeqCst);
            value * 2
        });

        assert_eq!(doubled, vec![2, 4, 6, 8, 10, 12]);
        assert!((2..=3).contains(&maximum_active.load(Ordering::SeqCst)));
    }
}
