use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    audit::{PrivacyFinding, Severity},
    budget::{BudgetExclusion, BudgetPlanner, BudgetPolicy},
    chunk::{estimate_tokens, ChunkKind},
    config::OutputConfigValues,
    corpus::{load_corpus, ExtractionIssue},
    rank::{rank_chunks, QueryTerms, RankedChunk, ScoreBreakdown},
    scanner::ScanOptions,
    ContextForgeError, Result,
};

pub const BUNDLE_FILE: &str = "context-bundle.md";
pub const MANIFEST_FILE: &str = "context-manifest.json";
pub const REPORT_FILE: &str = "context-report.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackOptions {
    pub redact: bool,
    pub fail_on: Option<Severity>,
    pub scan_options: crate::scanner::ScanOptions,
    pub file_names: PackFileNames,
    pub write_outputs: bool,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            redact: false,
            fail_on: None,
            scan_options: crate::scanner::ScanOptions::default(),
            file_names: PackFileNames::default(),
            write_outputs: true,
        }
    }
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
    pub selected_privacy_findings: Vec<PrivacyFinding>,
    pub extraction_issues: Vec<ExtractionIssue>,
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
    selected_chunks: Vec<ManifestSelectedChunk<'a>>,
    excluded_chunks_total: usize,
    excluded_chunks: Vec<ManifestExcludedChunk<'a>>,
    privacy_findings_total: usize,
    privacy_findings: Vec<ManifestPrivacyFinding>,
    selected_privacy_findings: Vec<ManifestPrivacyFinding>,
    extraction_issues: Vec<ManifestExtractionIssue<'a>>,
}

#[derive(Debug, Serialize)]
struct ManifestSelectedChunk<'a> {
    path: String,
    kind: &'static str,
    title: Option<&'a str>,
    start_line: usize,
    end_line: usize,
    score: usize,
    token_estimate: usize,
    score_breakdown: &'a ScoreBreakdown,
    redacted: bool,
}

#[derive(Debug, Serialize)]
struct ManifestExcludedChunk<'a> {
    path: String,
    start_line: usize,
    end_line: usize,
    score: usize,
    token_estimate: usize,
    reason: &'a str,
}

#[derive(Debug, Serialize)]
struct ManifestPrivacyFinding {
    path: String,
    line: usize,
    kind: String,
    severity: String,
    evidence: String,
}

#[derive(Debug, Serialize)]
struct ManifestExtractionIssue<'a> {
    path: String,
    message: &'a str,
}

struct RenderContext<'a> {
    goal: &'a str,
    source: &'a Path,
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
    selected_findings: &'a [PrivacyFinding],
    extraction_issues: &'a [ExtractionIssue],
}

struct BundleGroup<'a> {
    path: String,
    chunks: Vec<&'a PackedChunk>,
}

struct BundleSpan {
    start_line: usize,
    end_line: usize,
    text: String,
    redacted: bool,
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
    let goal = goal.trim();
    if goal.is_empty() {
        return Err(ContextForgeError::InvalidGoal);
    }
    if budget == 0 {
        return Err(ContextForgeError::InvalidBudget);
    }

    let fixed_overhead = bundle_fixed_overhead(goal);
    if budget <= fixed_overhead {
        return Err(ContextForgeError::BudgetTooSmall {
            minimum: fixed_overhead + 1,
        });
    }

    let scan_options = pack_scan_options(source, output_dir, &options.scan_options);
    let corpus = load_corpus(source, &scan_options)?;
    let terms = QueryTerms::parse(goal);
    let ranked_chunks = rank_chunks(corpus.chunks, &terms);
    if ranked_chunks.is_empty() {
        return Err(ContextForgeError::NoMatchingContext {
            goal: goal.to_string(),
        });
    }

    let candidate_count = ranked_chunks.len();
    let (mut ranked_chunks, relevance_exclusions) = apply_relevance_floor(ranked_chunks);
    add_rendering_costs(source, &mut ranked_chunks);
    let minimum_bundle_budget = fixed_overhead
        + ranked_chunks
            .iter()
            .map(|chunk| chunk.token_estimate)
            .min()
            .unwrap_or(1);
    let budget_policy = BudgetPolicy::new(budget - fixed_overhead);
    let mut budget_plan = BudgetPlanner::new(budget_policy).select(ranked_chunks);
    budget_plan.excluded.extend(relevance_exclusions);
    let privacy_findings = corpus.privacy_findings;
    let extraction_issues = corpus.extraction_issues;
    if budget_plan.selected.is_empty() {
        return Err(ContextForgeError::BudgetTooSmall {
            minimum: minimum_bundle_budget,
        });
    }

    let selected_privacy_findings =
        selected_privacy_findings(&privacy_findings, &budget_plan.selected);
    validate_privacy_gate(&selected_privacy_findings, options.fail_on)?;

    let used_tokens = fixed_overhead + budget_plan.used_tokens;
    let remaining_tokens = budget.saturating_sub(used_tokens);
    let selected_chunks = budget_plan
        .selected
        .into_iter()
        .map(|chunk| pack_chunk(chunk, &privacy_findings, options.redact))
        .collect::<Vec<_>>();
    let redaction_enabled = options.redact;
    let redacted = selected_chunks.iter().any(|chunk| chunk.redacted);
    let excluded_chunks = budget_plan.excluded;
    let statistics = build_statistics(&selected_chunks, &selected_privacy_findings);

    let bundle_path = output_dir.join(&options.file_names.bundle);
    let manifest_path = output_dir.join(&options.file_names.manifest);
    let report_path = output_dir.join(&options.file_names.report);

    if options.write_outputs {
        fs::create_dir_all(output_dir).map_err(|source| ContextForgeError::WriteOutput {
            path: output_dir.to_path_buf(),
            source,
        })?;

        let render_context = RenderContext {
            goal,
            source,
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
            selected_findings: &selected_privacy_findings,
            extraction_issues: &extraction_issues,
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
        selected_privacy_findings,
        extraction_issues,
    })
}

fn selected_privacy_findings(
    findings: &[PrivacyFinding],
    chunks: &[RankedChunk],
) -> Vec<PrivacyFinding> {
    findings
        .iter()
        .filter(|finding| {
            chunks.iter().any(|chunk| {
                chunk.path == finding.path
                    && (chunk.start_line..=chunk.end_line).contains(&finding.line)
            })
        })
        .cloned()
        .collect()
}

fn bundle_fixed_overhead(goal: &str) -> usize {
    estimate_tokens(&format!(
        "# Context Bundle\n\n## Goal\n\n{goal}\n\n## Selected Context\n\n"
    ))
}

fn add_rendering_costs(source: &Path, chunks: &mut [RankedChunk]) {
    for chunk in chunks {
        let label = format!(
            "### {}\n\nLines {}-{}\n\n",
            display_source_path(source, &chunk.path),
            chunk.start_line,
            chunk.end_line
        );
        chunk.token_estimate += estimate_tokens(&label);
    }
}

fn apply_relevance_floor(
    ranked_chunks: Vec<RankedChunk>,
) -> (Vec<RankedChunk>, Vec<BudgetExclusion>) {
    let file_minimum_score = ranked_chunks
        .first()
        .map(|chunk| chunk.score.div_ceil(4))
        .unwrap_or_default();
    let dominant_file_score = ranked_chunks
        .first()
        .map(|chunk| chunk.score.div_ceil(2))
        .unwrap_or_default();
    let mut file_signals = BTreeMap::<PathBuf, (usize, bool)>::new();
    for chunk in &ranked_chunks {
        let metadata_match = chunk.score_breakdown.path_match_score > 0
            || chunk.score_breakdown.title_match_score > 0
            || chunk.score_breakdown.file_name_match_score > 0;
        file_signals
            .entry(chunk.path.clone())
            .and_modify(|(score, matched)| {
                if chunk.score > *score {
                    *score = chunk.score;
                    *matched = metadata_match;
                } else if chunk.score == *score {
                    *matched |= metadata_match;
                }
            })
            .or_insert((chunk.score, metadata_match));
    }
    let mut included = Vec::new();
    let mut excluded = Vec::new();

    for chunk in ranked_chunks {
        let (strongest_file_score, metadata_match) =
            file_signals.get(&chunk.path).copied().unwrap_or_default();
        let local_minimum_score = strongest_file_score.div_ceil(10);
        let trusted_file = strongest_file_score >= file_minimum_score
            && (metadata_match || strongest_file_score >= dominant_file_score);
        if trusted_file && chunk.score >= local_minimum_score {
            included.push(chunk);
            continue;
        }

        excluded.push(BudgetExclusion {
            path: chunk.path,
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            score: chunk.score,
            token_estimate: chunk.token_estimate,
            preview: chunk.preview,
            reason: format!(
                "automatic relevance floor: score {}, file best {strongest_file_score}, file minimum {file_minimum_score}, dominant minimum {dominant_file_score}, metadata match {metadata_match}, local minimum {local_minimum_score}",
                chunk.score,
            ),
        });
    }

    (included, excluded)
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
    output.push_str("## Selected Context\n\n");

    if context.chunks.is_empty() {
        output.push_str("No matching context was selected.\n\n");
    } else {
        for group in bundle_groups(context.source, context.chunks) {
            output.push_str(&format!("### {}\n\n", group.path));
            for span in bundle_spans(&group.chunks) {
                output.push_str(&format!(
                    "Lines {}-{}\n\n{}\n\n",
                    span.start_line, span.end_line, span.text
                ));
            }
        }
    }

    output
}

fn bundle_spans(chunks: &[&PackedChunk]) -> Vec<BundleSpan> {
    let mut spans = Vec::<BundleSpan>::new();

    for chunk in chunks {
        if let Some(span) = spans.last_mut() {
            if chunk.start_line > span.end_line
                && chunk.start_line <= span.end_line.saturating_add(2)
            {
                span.end_line = span.end_line.max(chunk.end_line);
                span.text.push_str("\n\n");
                span.text.push_str(&chunk.text);
                span.redacted |= chunk.redacted;
                continue;
            }
        }

        spans.push(BundleSpan {
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            text: chunk.text.clone(),
            redacted: chunk.redacted,
        });
    }

    spans
}

fn bundle_groups<'a>(source: &Path, chunks: &'a [PackedChunk]) -> Vec<BundleGroup<'a>> {
    let mut groups = Vec::<BundleGroup<'a>>::new();

    for chunk in chunks {
        let path = display_source_path(source, &chunk.path);
        if let Some(group) = groups.iter_mut().find(|group| group.path == path) {
            group.chunks.push(chunk);
            continue;
        }

        groups.push(BundleGroup {
            path,
            chunks: vec![chunk],
        });
    }

    for group in &mut groups {
        group
            .chunks
            .sort_by_key(|chunk| (chunk.start_line, chunk.end_line));
    }

    groups
}

fn render_manifest(context: &RenderContext<'_>) -> Result<String> {
    const DETAIL_LIMIT: usize = 50;
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
        selected_chunks: context
            .chunks
            .iter()
            .map(|chunk| ManifestSelectedChunk {
                path: crate::paths::relative_display(context.source, &chunk.path),
                kind: chunk.kind.label(),
                title: chunk.title.as_deref(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                score: chunk.score,
                token_estimate: chunk.token_estimate,
                score_breakdown: &chunk.score_breakdown,
                redacted: chunk.redacted,
            })
            .collect(),
        excluded_chunks_total: context.excluded_chunks.len(),
        excluded_chunks: context
            .excluded_chunks
            .iter()
            .take(DETAIL_LIMIT)
            .map(|chunk| ManifestExcludedChunk {
                path: crate::paths::relative_display(context.source, &chunk.path),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                score: chunk.score,
                token_estimate: chunk.token_estimate,
                reason: &chunk.reason,
            })
            .collect(),
        privacy_findings_total: context.findings.len(),
        privacy_findings: context
            .findings
            .iter()
            .take(DETAIL_LIMIT)
            .map(|finding| ManifestPrivacyFinding {
                path: crate::paths::relative_display(context.source, &finding.path),
                line: finding.line,
                kind: finding.kind.label().to_string(),
                severity: finding.severity.label().to_string(),
                evidence: finding.evidence.clone(),
            })
            .collect(),
        selected_privacy_findings: context
            .selected_findings
            .iter()
            .map(|finding| ManifestPrivacyFinding {
                path: crate::paths::relative_display(context.source, &finding.path),
                line: finding.line,
                kind: finding.kind.label().to_string(),
                severity: finding.severity.label().to_string(),
                evidence: finding.evidence.clone(),
            })
            .collect(),
        extraction_issues: context
            .extraction_issues
            .iter()
            .map(|issue| ManifestExtractionIssue {
                path: crate::paths::relative_display(context.source, &issue.path),
                message: &issue.message,
            })
            .collect(),
    };

    serde_json::to_string_pretty(&manifest)
        .map_err(|source| ContextForgeError::SerializeManifest { source })
}

fn render_report(context: &RenderContext<'_>) -> String {
    let groups = bundle_groups(context.source, context.chunks);
    let selected_files = groups.len();
    let spans = groups
        .iter()
        .flat_map(|group| bundle_spans(&group.chunks))
        .collect::<Vec<_>>();
    let selected_spans = spans.len();
    let redacted_spans = spans.iter().filter(|span| span.redacted).count();
    format!(
        "# ContextForge Report\n\n- Goal: {goal}\n- Budget: {budget}\n- Used tokens: {used_tokens}\n- Remaining tokens: {remaining_tokens}\n- Per-file budget limit: {}\n- Candidate chunks: {candidate_count}\n- Selected files: {selected_files}\n- Selected spans: {}\n- Excluded chunks: {}\n- Selected privacy findings: {}\n- Source privacy findings: {}\n- Extraction warnings: {}\n- Redaction: {}\n- Redacted spans: {}\n\n## Selected privacy severity counts\n\n{}\n## Selected privacy finding types\n\n{}",
        context.budget_policy.per_file_budget_limit(),
        selected_spans,
        context.excluded_chunks.len(),
        context.selected_findings.len(),
        context.findings.len(),
        context.extraction_issues.len(),
        if context.redaction_enabled {
            "enabled"
        } else {
            "disabled"
        },
        redacted_spans,
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

fn pack_scan_options(source: &Path, output_dir: &Path, base_options: &ScanOptions) -> ScanOptions {
    let mut scan_options = base_options.clone();
    let source = crate::paths::absolute(source);
    let output_dir = crate::paths::absolute(output_dir);

    if output_dir != source && output_dir.starts_with(&source) {
        if let Some(name) = output_dir.file_name().and_then(|name| name.to_str()) {
            let name = name.to_string();
            if !scan_options.ignored_directories.contains(&name) {
                scan_options.ignored_directories.push(name);
            }
        }
    }

    scan_options
}

fn display_source_path(source: &Path, path: &Path) -> String {
    crate::paths::relative_display(source, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rank::ScoreBreakdown;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn pack_directory_ignores_existing_output_directory_inside_source() {
        let temp = tempdir().expect("temporary directory");
        let source = temp.path().join("source");
        let output = source.join("out");
        fs::create_dir_all(source.join("docs")).expect("docs directory");
        fs::create_dir_all(&output).expect("output directory");
        fs::write(
            source.join("docs/requirements.md"),
            "fresh target belongs in the source document\n",
        )
        .expect("source document");
        fs::write(
            output.join("context-bundle.md"),
            "fresh target from a previous generated bundle\n",
        )
        .expect("stale bundle");

        let result = pack_directory_with_options(
            &source,
            "fresh target",
            200,
            &output,
            PackOptions {
                write_outputs: false,
                ..PackOptions::default()
            },
        )
        .expect("pack result");

        assert!(!result
            .selected_chunks
            .iter()
            .any(|chunk| chunk.path.ends_with("context-bundle.md")));
        assert!(result
            .selected_chunks
            .iter()
            .any(|chunk| chunk.path.ends_with("requirements.md")));
    }

    #[test]
    fn relevance_floor_keeps_supporting_chunks_from_a_strong_file() {
        let chunks = vec![
            ranked_chunk("docs/strong.md", 100),
            ranked_chunk("docs/strong.md", 20),
            ranked_chunk("docs/weak.md", 20),
        ];

        let (included, excluded) = apply_relevance_floor(chunks);

        assert_eq!(included.len(), 2);
        assert!(included
            .iter()
            .all(|chunk| chunk.path.ends_with("strong.md")));
        assert_eq!(excluded.len(), 1);
        assert!(excluded[0].path.ends_with("weak.md"));
    }

    #[test]
    fn relevance_floor_keeps_metadata_anchor_and_rejects_lexical_noise() {
        let mut anchored = ranked_chunk("docs/anchored.md", 40);
        anchored.score_breakdown.title_match_score = 4;
        let chunks = vec![
            ranked_chunk("docs/top.md", 100),
            anchored,
            ranked_chunk("labs/noise.md", 40),
        ];

        let (included, excluded) = apply_relevance_floor(chunks);

        assert_eq!(included.len(), 2);
        assert!(included.iter().any(|chunk| chunk.path.ends_with("top.md")));
        assert!(included
            .iter()
            .any(|chunk| chunk.path.ends_with("anchored.md")));
        assert_eq!(excluded.len(), 1);
        assert!(excluded[0].path.ends_with("noise.md"));
    }

    #[test]
    fn relevance_floor_does_not_promote_file_from_weaker_title_match() {
        let mut weak_title = ranked_chunk("labs/noise.md", 20);
        weak_title.score_breakdown.title_match_score = 4;
        let chunks = vec![
            ranked_chunk("docs/top.md", 100),
            ranked_chunk("labs/noise.md", 40),
            weak_title,
        ];

        let (included, excluded) = apply_relevance_floor(chunks);

        assert_eq!(included.len(), 1);
        assert!(included[0].path.ends_with("top.md"));
        assert_eq!(excluded.len(), 2);
        assert!(excluded
            .iter()
            .all(|chunk| chunk.path.ends_with("noise.md")));
    }

    fn ranked_chunk(path: &str, score: usize) -> RankedChunk {
        RankedChunk {
            path: PathBuf::from(path),
            kind: ChunkKind::Paragraph,
            title: None,
            start_line: 1,
            end_line: 1,
            score,
            token_estimate: 10,
            text: "course project requirements".to_string(),
            preview: "course project requirements".to_string(),
            score_breakdown: ScoreBreakdown {
                lexical_score: score,
                text_match_score: 0,
                term_coverage_score: 0,
                full_coverage_score: 0,
                path_match_score: 0,
                title_match_score: 0,
                file_name_match_score: 0,
                file_kind_score: 0,
                density_score: 0,
                total_score: score,
                reasons: Vec::new(),
            },
        }
    }
}
