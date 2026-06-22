use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::rank::RankedChunk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BudgetPolicy {
    pub max_tokens: usize,
    pub max_file_share_percent: usize,
}

impl BudgetPolicy {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            max_file_share_percent: 60,
        }
    }

    pub fn per_file_budget_limit(self) -> usize {
        ((self.max_tokens * self.max_file_share_percent) / 100).max(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BudgetExclusionKind {
    OverGlobalBudget,
    OverPerFileBudget,
}

impl BudgetExclusionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::OverGlobalBudget => "global budget limit",
            Self::OverPerFileBudget => "per-file budget limit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BudgetExclusion {
    pub path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub score: usize,
    pub token_estimate: usize,
    pub preview: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetPlan {
    pub policy: BudgetPolicy,
    pub candidate_count: usize,
    pub selected: Vec<RankedChunk>,
    pub excluded: Vec<BudgetExclusion>,
    pub used_tokens: usize,
    pub remaining_tokens: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetPlanner {
    policy: BudgetPolicy,
}

impl BudgetPlanner {
    pub fn new(policy: BudgetPolicy) -> Self {
        Self { policy }
    }

    pub fn select(self, candidates: Vec<RankedChunk>) -> BudgetPlan {
        let candidate_count = candidates.len();
        let mut selected = Vec::new();
        let mut deferred = Vec::new();
        let mut excluded = Vec::new();
        let mut used_tokens = 0;
        let mut file_usage = BTreeMap::<PathBuf, usize>::new();
        let per_file_limit = self.policy.per_file_budget_limit();

        for (index, candidate) in candidates.into_iter().enumerate() {
            let remaining = self.policy.max_tokens.saturating_sub(used_tokens);
            if candidate.token_estimate > remaining {
                excluded.push(exclusion(
                    candidate,
                    BudgetExclusionKind::OverGlobalBudget,
                    used_tokens,
                    self.policy.max_tokens,
                ));
                continue;
            }

            let current_file_usage = file_usage_for(&file_usage, &candidate.path);
            if current_file_usage > 0
                && current_file_usage + candidate.token_estimate > per_file_limit
            {
                deferred.push((index, candidate, current_file_usage));
                continue;
            }

            used_tokens += candidate.token_estimate;
            *file_usage.entry(candidate.path.clone()).or_default() += candidate.token_estimate;
            selected.push((index, candidate));
        }

        for (index, candidate, deferred_file_usage) in deferred {
            let remaining = self.policy.max_tokens.saturating_sub(used_tokens);
            if candidate.token_estimate > remaining {
                excluded.push(exclusion(
                    candidate,
                    BudgetExclusionKind::OverPerFileBudget,
                    deferred_file_usage,
                    per_file_limit,
                ));
                continue;
            }

            used_tokens += candidate.token_estimate;
            selected.push((index, candidate));
        }

        selected.sort_by_key(|(index, _)| *index);
        let selected = selected
            .into_iter()
            .map(|(_, candidate)| candidate)
            .collect();

        BudgetPlan {
            policy: self.policy,
            candidate_count,
            selected,
            excluded,
            used_tokens,
            remaining_tokens: self.policy.max_tokens.saturating_sub(used_tokens),
        }
    }
}

fn file_usage_for(file_usage: &BTreeMap<PathBuf, usize>, path: &Path) -> usize {
    file_usage.get(path).copied().unwrap_or_default()
}

fn exclusion(
    candidate: RankedChunk,
    kind: BudgetExclusionKind,
    current_tokens: usize,
    limit: usize,
) -> BudgetExclusion {
    BudgetExclusion {
        path: candidate.path,
        start_line: candidate.start_line,
        end_line: candidate.end_line,
        score: candidate.score,
        token_estimate: candidate.token_estimate,
        preview: candidate.preview,
        reason: format!(
            "{}: current {current_tokens}, candidate {}, limit {limit}",
            kind.label(),
            candidate.token_estimate
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rank::ScoreBreakdown;

    #[test]
    fn select_records_per_file_budget_exclusions() {
        let chunks = vec![
            ranked("docs/a.md", 20, 30),
            ranked("docs/a.md", 18, 25),
            ranked("docs/a.md", 16, 30),
            ranked("docs/b.md", 14, 20),
        ];
        let planner = BudgetPlanner::new(BudgetPolicy::new(100));

        let plan = planner.select(chunks);

        assert_eq!(plan.used_tokens, 75);
        assert_eq!(plan.selected.len(), 3);
        assert_eq!(plan.excluded.len(), 1);
        assert!(plan.excluded[0].reason.contains("per-file budget limit"));
    }

    #[test]
    fn select_fills_unused_budget_from_one_relevant_file() {
        let chunks = vec![
            ranked("docs/a.md", 20, 30),
            ranked("docs/a.md", 18, 25),
            ranked("docs/a.md", 16, 24),
        ];
        let planner = BudgetPlanner::new(BudgetPolicy::new(100));

        let plan = planner.select(chunks);

        assert_eq!(plan.used_tokens, 79);
        assert_eq!(plan.selected.len(), 3);
        assert!(plan.excluded.is_empty());
    }

    fn ranked(path: &str, score: usize, tokens: usize) -> RankedChunk {
        RankedChunk {
            path: PathBuf::from(path),
            kind: crate::chunk::ChunkKind::MarkdownSection,
            title: Some("Ownership".to_string()),
            start_line: 1,
            end_line: 1,
            score,
            token_estimate: tokens,
            text: "ownership borrowing".to_string(),
            preview: "ownership borrowing".to_string(),
            score_breakdown: ScoreBreakdown {
                lexical_score: score,
                text_match_score: score,
                term_coverage_score: 0,
                full_coverage_score: 0,
                path_match_score: 0,
                title_match_score: 0,
                file_name_match_score: 0,
                file_kind_score: 0,
                density_score: 0,
                total_score: score,
                reasons: vec!["text matches: 2 x 3".to_string()],
            },
        }
    }
}
