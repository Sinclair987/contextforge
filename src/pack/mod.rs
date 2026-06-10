use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    audit::{audit_directory, PrivacyFinding},
    search::{search_directory, SearchHit},
    ContextForgeError, Result,
};

pub const BUNDLE_FILE: &str = "context-bundle.md";
pub const MANIFEST_FILE: &str = "context-manifest.json";
pub const REPORT_FILE: &str = "context-report.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackResult {
    pub bundle_path: PathBuf,
    pub manifest_path: PathBuf,
    pub report_path: PathBuf,
    pub used_tokens: usize,
    pub selected_chunks: Vec<PackedChunk>,
    pub privacy_findings: Vec<PrivacyFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackedChunk {
    pub path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub score: usize,
    pub token_estimate: usize,
    pub text: String,
    pub preview: String,
}

#[derive(Debug, Serialize)]
struct PackManifest<'a> {
    goal: &'a str,
    budget: usize,
    used_tokens: usize,
    selected_chunks: &'a [PackedChunk],
    privacy_findings: Vec<ManifestPrivacyFinding>,
    excluded_files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ManifestPrivacyFinding {
    path: PathBuf,
    line: usize,
    kind: String,
    severity: String,
    evidence: String,
}

pub fn pack_directory(
    source: &Path,
    goal: &str,
    budget: usize,
    output_dir: &Path,
) -> Result<PackResult> {
    let selected_chunks = select_chunks(search_directory(source, goal)?, budget);
    let privacy_findings = audit_directory(source)?;
    let used_tokens = selected_chunks
        .iter()
        .map(|chunk| chunk.token_estimate)
        .sum();

    fs::create_dir_all(output_dir).map_err(|source| ContextForgeError::WriteOutput {
        path: output_dir.to_path_buf(),
        source,
    })?;

    let bundle_path = output_dir.join(BUNDLE_FILE);
    let manifest_path = output_dir.join(MANIFEST_FILE);
    let report_path = output_dir.join(REPORT_FILE);

    write_output(
        &bundle_path,
        &render_bundle(
            goal,
            budget,
            used_tokens,
            &selected_chunks,
            &privacy_findings,
        ),
    )?;
    write_output(
        &manifest_path,
        &render_manifest(
            goal,
            budget,
            used_tokens,
            &selected_chunks,
            &privacy_findings,
        )?,
    )?;
    write_output(
        &report_path,
        &render_report(
            goal,
            budget,
            used_tokens,
            &selected_chunks,
            &privacy_findings,
        ),
    )?;

    Ok(PackResult {
        bundle_path,
        manifest_path,
        report_path,
        used_tokens,
        selected_chunks,
        privacy_findings,
    })
}

fn select_chunks(hits: Vec<SearchHit>, budget: usize) -> Vec<PackedChunk> {
    let mut remaining = budget;
    let mut selected = Vec::new();

    for hit in hits {
        if hit.token_estimate > remaining {
            continue;
        }

        remaining -= hit.token_estimate;
        selected.push(PackedChunk {
            path: hit.path,
            start_line: hit.start_line,
            end_line: hit.end_line,
            score: hit.score,
            token_estimate: hit.token_estimate,
            text: hit.text,
            preview: hit.preview,
        });
    }

    selected
}

fn render_bundle(
    goal: &str,
    budget: usize,
    used_tokens: usize,
    chunks: &[PackedChunk],
    findings: &[PrivacyFinding],
) -> String {
    let mut output = String::new();
    output.push_str("# Context Bundle\n\n");
    output.push_str("## Goal\n\n");
    output.push_str(goal);
    output.push_str("\n\n");
    output.push_str("## Budget\n\n");
    output.push_str(&format!(
        "- Budget: {budget}\n- Used tokens: {used_tokens}\n\n"
    ));
    output.push_str("## Selected Context\n\n");

    if chunks.is_empty() {
        output.push_str("No matching context was selected.\n\n");
    } else {
        for chunk in chunks {
            output.push_str(&format!(
                "### {}: lines {}-{}\n\n{}\n\n",
                chunk.path.display(),
                chunk.start_line,
                chunk.end_line,
                chunk.text
            ));
        }
    }

    output.push_str("## Privacy findings\n\n");
    if findings.is_empty() {
        output.push_str("No privacy risks found.\n");
    } else {
        for finding in findings {
            output.push_str(&format!(
                "- {} | {} | {}: line {} | {}\n",
                finding.severity.label(),
                finding.kind.label(),
                finding.path.display(),
                finding.line,
                finding.evidence
            ));
        }
    }

    output
}

fn render_manifest(
    goal: &str,
    budget: usize,
    used_tokens: usize,
    chunks: &[PackedChunk],
    findings: &[PrivacyFinding],
) -> Result<String> {
    let manifest = PackManifest {
        goal,
        budget,
        used_tokens,
        selected_chunks: chunks,
        privacy_findings: findings
            .iter()
            .map(|finding| ManifestPrivacyFinding {
                path: finding.path.clone(),
                line: finding.line,
                kind: finding.kind.label().to_string(),
                severity: finding.severity.label().to_string(),
                evidence: finding.evidence.clone(),
            })
            .collect(),
        excluded_files: Vec::new(),
    };

    serde_json::to_string_pretty(&manifest)
        .map_err(|source| ContextForgeError::SerializeManifest { source })
}

fn render_report(
    goal: &str,
    budget: usize,
    used_tokens: usize,
    chunks: &[PackedChunk],
    findings: &[PrivacyFinding],
) -> String {
    format!(
        "# ContextForge Report\n\n- Goal: {goal}\n- Budget: {budget}\n- Used tokens: {used_tokens}\n- Selected chunks: {}\n- Privacy findings: {}\n",
        chunks.len(),
        findings.len()
    )
}

fn write_output(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).map_err(|source| ContextForgeError::WriteOutput {
        path: path.to_path_buf(),
        source,
    })
}
