use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    audit::{audit_directory_with_options, PrivacyFinding, Severity},
    budget::{BudgetExclusion, BudgetPlanner, BudgetPolicy},
    chunk::ChunkKind,
    config::OutputConfigValues,
    rank::{RankedChunk, ScoreBreakdown},
    ContextForgeError, Result,
};

pub const BUNDLE_FILE: &str = "context-bundle.md";
pub const MANIFEST_FILE: &str = "context-manifest.json";
pub const REPORT_FILE: &str = "context-report.md";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackOptions {
    pub redact: bool,
    pub fail_on: Option<Severity>,
    pub scan_options: crate::scanner::ScanOptions,
    pub file_names: PackFileNames,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFileNames {
    pub bundle: String,
    pub manifest: String,
    pub report: String,
}

impl Default for PackFileNames {
    fn default() -> Self {
        Self {
            bundle: BUNDLE_FILE.to_string(),
            manifest: MANIFEST_FILE.to_string(),
            report: REPORT_FILE.to_string(),
        }
    }
}

impl From<OutputConfigValues> for PackFileNames {
    fn from(value: OutputConfigValues) -> Self {
        Self {
            bundle: value.bundle,
            manifest: value.manifest,
            report: value.report,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackResult {
    pub bundle_path: PathBuf,
    pub manifest_path: PathBuf,
    pub report_path: PathBuf,
    pub used_tokens: usize,
    pub remaining_tokens: usize,
    pub budget_policy: BudgetPolicy,
    pub redaction_enabled: bool,
    pub redacted: bool,
    pub selected_chunks: Vec<PackedChunk>,
    pub excluded_chunks: Vec<BudgetExclusion>,
    pub privacy_findings: Vec<PrivacyFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackedChunk {
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
    pub selection_reason: String,
    pub redacted: bool,
}

#[derive(Debug, Serialize)]
struct PackManifest<'a> {
    goal: &'a str,
    budget: usize,
    used_tokens: usize,
    remaining_tokens: usize,
    per_file_budget_limit: usize,
    candidate_chunks: usize,
    redaction_enabled: bool,
    redacted: bool,
    selected_chunk_types: BTreeMap<String, usize>,
    privacy_severity_counts: BTreeMap<String, usize>,
    privacy_kind_counts: BTreeMap<String, usize>,
    selected_chunks: &'a [PackedChunk],
    excluded_chunks: &'a [BudgetExclusion],
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

struct RenderContext<'a> {
    goal: &'a str,
    budget: usize,
    used_tokens: usize,
    remaining_tokens: usize,
    budget_policy: BudgetPolicy,
    candidate_count: usize,
    redaction_enabled: bool,
    redacted: bool,
    statistics: PackStatistics,
    chunks: &'a [PackedChunk],
    excluded_chunks: &'a [BudgetExclusion],
    findings: &'a [PrivacyFinding],
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
struct PackStatistics {
    selected_chunk_types: BTreeMap<String, usize>,
    privacy_severity_counts: BTreeMap<String, usize>,
    privacy_kind_counts: BTreeMap<String, usize>,
}

pub fn pack_directory(
    source: &Path,
    goal: &str,
    budget: usize,
    output_dir: &Path,
) -> Result<PackResult> {
    pack_directory_with_options(source, goal, budget, output_dir, PackOptions::default())
}

pub fn pack_directory_with_options(
    source: &Path,
    goal: &str,
    budget: usize,
    output_dir: &Path,
    options: PackOptions,
) -> Result<PackResult> {
    let ranked_chunks =
        crate::rank::rank_directory_with_options(source, goal, &options.scan_options)?;
    let candidate_count = ranked_chunks.len();
    let budget_policy = BudgetPolicy::new(budget);
    let budget_plan = BudgetPlanner::new(budget_policy).select(ranked_chunks);
    let privacy_findings = audit_directory_with_options(source, &options.scan_options)?;
    validate_privacy_gate(&privacy_findings, options.fail_on)?;

    let used_tokens = budget_plan.used_tokens;
    let remaining_tokens = budget_plan.remaining_tokens;
    let selected_chunks = budget_plan
        .selected
        .into_iter()
        .map(|chunk| pack_chunk(chunk, &privacy_findings, options.redact))
        .collect::<Vec<_>>();
    let redaction_enabled = options.redact;
    let redacted = selected_chunks.iter().any(|chunk| chunk.redacted);
    let excluded_chunks = budget_plan.excluded;
    let statistics = build_statistics(&selected_chunks, &privacy_findings);

    fs::create_dir_all(output_dir).map_err(|source| ContextForgeError::WriteOutput {
        path: output_dir.to_path_buf(),
        source,
    })?;

    let bundle_path = output_dir.join(&options.file_names.bundle);
    let manifest_path = output_dir.join(&options.file_names.manifest);
    let report_path = output_dir.join(&options.file_names.report);

    {
        let render_context = RenderContext {
            goal,
            budget,
            used_tokens,
            remaining_tokens,
            budget_policy,
            candidate_count,
            redaction_enabled,
            redacted,
            statistics,
            chunks: &selected_chunks,
            excluded_chunks: &excluded_chunks,
            findings: &privacy_findings,
        };

        write_output(&bundle_path, &render_bundle(&render_context))?;
        write_output(&manifest_path, &render_manifest(&render_context)?)?;
        write_output(&report_path, &render_report(&render_context))?;
    }

    Ok(PackResult {
        bundle_path,
        manifest_path,
        report_path,
        used_tokens,
        remaining_tokens,
        budget_policy,
        redaction_enabled,
        redacted,
        selected_chunks,
        excluded_chunks,
        privacy_findings,
    })
}

fn pack_chunk(chunk: RankedChunk, findings: &[PrivacyFinding], redact: bool) -> PackedChunk {
    let selection_reason = format!(
        "selected within global and per-file budgets; {}",
        chunk.score_breakdown.summary()
    );
    let (text, redacted) = if redact {
        redact_chunk_text(&chunk, findings)
    } else {
        (chunk.text, false)
    };
    let preview = if redacted {
        preview(&text)
    } else {
        chunk.preview
    };

    PackedChunk {
        path: chunk.path,
        kind: chunk.kind,
        title: chunk.title,
        start_line: chunk.start_line,
        end_line: chunk.end_line,
        score: chunk.score,
        token_estimate: chunk.token_estimate,
        text,
        preview,
        score_breakdown: chunk.score_breakdown,
        selection_reason,
        redacted,
    }
}

fn render_bundle(context: &RenderContext<'_>) -> String {
    let mut output = String::new();
    output.push_str("# Context Bundle\n\n");
    output.push_str("## Goal\n\n");
    output.push_str(context.goal);
    output.push_str("\n\n");
    output.push_str("## Budget\n\n");
    output.push_str(&format!(
        "- Budget: {budget}\n- Used tokens: {used_tokens}\n- Remaining tokens: {remaining_tokens}\n- Per-file budget limit: {}\n- Excluded chunks: {}\n\n",
        context.budget_policy.per_file_budget_limit(),
        context.excluded_chunks.len(),
        budget = context.budget,
        used_tokens = context.used_tokens,
        remaining_tokens = context.remaining_tokens
    ));
    output.push_str(&format!(
        "- Redaction: {}\n\n",
        if context.redaction_enabled {
            "enabled"
        } else {
            "disabled"
        }
    ));
    output.push_str("## Selected Context\n\n");

    if context.chunks.is_empty() {
        output.push_str("No matching context was selected.\n\n");
    } else {
        for chunk in context.chunks {
            let title = chunk
                .title
                .as_deref()
                .map(|title| format!(" | {title}"))
                .unwrap_or_default();
            output.push_str(&format!(
                "### {}: lines {}-{} | {}{}\n\n{}\n\n",
                chunk.path.display(),
                chunk.start_line,
                chunk.end_line,
                chunk.kind.label(),
                title,
                chunk.text
            ));
            output.push_str(&format!(
                "Score: {}\n\nSelection reason: {}\n\n",
                chunk.score, chunk.selection_reason
            ));
        }
    }

    output.push_str("## Privacy findings\n\n");
    if context.findings.is_empty() {
        output.push_str("No privacy risks found.\n");
    } else {
        for finding in context.findings {
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

fn render_manifest(context: &RenderContext<'_>) -> Result<String> {
    let manifest = PackManifest {
        goal: context.goal,
        budget: context.budget,
        used_tokens: context.used_tokens,
        remaining_tokens: context.remaining_tokens,
        per_file_budget_limit: context.budget_policy.per_file_budget_limit(),
        candidate_chunks: context.candidate_count,
        redaction_enabled: context.redaction_enabled,
        redacted: context.redacted,
        selected_chunk_types: context.statistics.selected_chunk_types.clone(),
        privacy_severity_counts: context.statistics.privacy_severity_counts.clone(),
        privacy_kind_counts: context.statistics.privacy_kind_counts.clone(),
        selected_chunks: context.chunks,
        excluded_chunks: context.excluded_chunks,
        privacy_findings: context
            .findings
            .iter()
            .map(|finding| ManifestPrivacyFinding {
                path: finding.path.clone(),
                line: finding.line,
                kind: finding.kind.label().to_string(),
                severity: finding.severity.label().to_string(),
                evidence: finding.evidence.clone(),
            })
            .collect(),
        excluded_files: excluded_files(context.excluded_chunks),
    };

    serde_json::to_string_pretty(&manifest)
        .map_err(|source| ContextForgeError::SerializeManifest { source })
}

fn render_report(context: &RenderContext<'_>) -> String {
    format!(
        "# ContextForge Report\n\n- Goal: {goal}\n- Budget: {budget}\n- Used tokens: {used_tokens}\n- Remaining tokens: {remaining_tokens}\n- Per-file budget limit: {}\n- Candidate chunks: {candidate_count}\n- Selected chunks: {}\n- Excluded chunks: {}\n- Privacy findings: {}\n- Redaction: {}\n- Redacted chunks: {}\n\n## Selected chunk types\n\n{}\n## Privacy severity counts\n\n{}\n## Privacy finding types\n\n{}",
        context.budget_policy.per_file_budget_limit(),
        context.chunks.len(),
        context.excluded_chunks.len(),
        context.findings.len(),
        if context.redaction_enabled {
            "enabled"
        } else {
            "disabled"
        },
        context.chunks.iter().filter(|chunk| chunk.redacted).count(),
        render_counts(&context.statistics.selected_chunk_types),
        render_counts(&context.statistics.privacy_severity_counts),
        render_counts(&context.statistics.privacy_kind_counts),
        goal = context.goal,
        budget = context.budget,
        used_tokens = context.used_tokens,
        remaining_tokens = context.remaining_tokens,
        candidate_count = context.candidate_count
    )
}

fn build_statistics(chunks: &[PackedChunk], findings: &[PrivacyFinding]) -> PackStatistics {
    let mut statistics = PackStatistics::default();

    for chunk in chunks {
        *statistics
            .selected_chunk_types
            .entry(chunk.kind.label().to_string())
            .or_default() += 1;
    }

    for finding in findings {
        *statistics
            .privacy_severity_counts
            .entry(finding.severity.label().to_string())
            .or_default() += 1;
        *statistics
            .privacy_kind_counts
            .entry(finding.kind.label().to_string())
            .or_default() += 1;
    }

    statistics
}

fn render_counts(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "- none: 0\n\n".to_string();
    }

    let mut output = String::new();
    for (label, count) in counts {
        output.push_str(&format!("- {label}: {count}\n"));
    }
    output.push('\n');
    output
}

fn write_output(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).map_err(|source| ContextForgeError::WriteOutput {
        path: path.to_path_buf(),
        source,
    })
}

fn excluded_files(excluded_chunks: &[BudgetExclusion]) -> Vec<String> {
    excluded_chunks
        .iter()
        .map(|chunk| format!("{}: {}", chunk.path.display(), chunk.reason))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn validate_privacy_gate(findings: &[PrivacyFinding], threshold: Option<Severity>) -> Result<()> {
    let Some(threshold) = threshold else {
        return Ok(());
    };

    let count = findings
        .iter()
        .filter(|finding| finding.severity.is_at_least(threshold))
        .count();

    if count == 0 {
        return Ok(());
    }

    Err(ContextForgeError::PrivacyGateFailed {
        severity: threshold.label().to_string(),
        count,
    })
}

fn redact_chunk_text(chunk: &RankedChunk, findings: &[PrivacyFinding]) -> (String, bool) {
    let mut redacted = false;
    let mut lines = Vec::new();

    for (offset, line) in chunk.text.lines().enumerate() {
        let source_line = chunk.start_line + offset;
        if let Some(finding) = findings
            .iter()
            .find(|finding| finding.path == chunk.path && finding.line == source_line)
        {
            redacted = true;
            lines.push(format!("[REDACTED: {}]", finding.kind.label()));
        } else {
            lines.push(line.to_string());
        }
    }

    (lines.join("\n"), redacted)
}

fn preview(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_PREVIEW_CHARS: usize = 160;
    if collapsed.chars().count() <= MAX_PREVIEW_CHARS {
        return collapsed;
    }

    collapsed.chars().take(MAX_PREVIEW_CHARS).collect()
}
