//! Deterministic attention signals for an analytical question.
//!
//! This is deliberately not a permission gate or semantic interpreter. The
//! model drives the analysis; these signals only tell the prompt and verifier
//! where extra care may be useful.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    Low,
    Elevated,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisRoute {
    ModelGuided,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub tier: RiskTier,
    pub route: AnalysisRoute,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<String>,
}

pub fn assess(question: &str) -> RiskAssessment {
    let lower = question.trim().to_ascii_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let mut signals = Vec::new();

    let high = [
        ("causal_claim", has_word(&words, "causal")),
        (
            "forecast",
            has_any_word(&words, &["forecast", "predict", "projection"]),
        ),
        ("explanation", has_word(&words, "why")),
        ("impact", has_word(&words, "impact")),
        (
            "correlation",
            has_any_word(&words, &["correlation", "correlate"]),
        ),
    ];
    for (signal, present) in high {
        if present {
            signals.push(signal.to_string());
        }
    }

    let elevated = [
        (
            "signed_values",
            has_any_word(
                &words,
                &[
                    "net", "spending", "spent", "refund", "refunds", "credit", "credits",
                ],
            ) || contains_any(
                &lower,
                &["excluding income", "after refunds", "including refunds"],
            ),
        ),
        (
            "time_series",
            contains_any(&lower, &["over time", "by month", "by week", "by day"]),
        ),
        (
            "comparison",
            has_any_word(
                &words,
                &["compare", "compared", "versus", "vs", "difference"],
            ),
        ),
        (
            "ratio",
            has_any_word(&words, &["ratio", "rate", "percentage", "percent", "share"]),
        ),
        (
            "distribution",
            has_any_word(&words, &["average", "median", "distribution", "outlier"]),
        ),
        (
            "ranking",
            has_any_word(
                &words,
                &["rank", "ranking", "top", "bottom", "highest", "lowest"],
            ),
        ),
        (
            "grouping",
            has_any_word(&words, &["group", "breakdown", "per", "each", "across"])
                || (has_word(&words, "by")
                    && !contains_any(&lower, &["by how much", "by how many"])),
        ),
        ("join", has_any_word(&words, &["join", "combine", "match"])),
        (
            "change",
            has_any_word(
                &words,
                &["change", "growth", "decline", "increase", "decrease"],
            ),
        ),
    ];
    for (signal, present) in elevated {
        if present && !signals.iter().any(|existing| existing == signal) {
            signals.push(signal.to_string());
        }
    }

    let tier = if high.iter().any(|(_, present)| *present) {
        RiskTier::High
    } else if !signals.is_empty() {
        RiskTier::Elevated
    } else {
        RiskTier::Low
    };
    RiskAssessment {
        tier,
        route: AnalysisRoute::ModelGuided,
        signals,
    }
}

fn has_word(words: &[&str], expected: &str) -> bool {
    words.contains(&expected)
}

fn has_any_word(words: &[&str], expected: &[&str]) -> bool {
    expected.iter().any(|word| has_word(words, word))
}

fn contains_any(text: &str, expected: &[&str]) -> bool {
    expected.iter().any(|phrase| text.contains(phrase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obvious_lookups_stay_model_guided() {
        let assessment = assess("How many rows are in sales?");
        assert_eq!(assessment.tier, RiskTier::Low);
        assert_eq!(assessment.route, AnalysisRoute::ModelGuided);
    }

    #[test]
    fn comparisons_and_time_series_raise_attention_signals() {
        let assessment = assess("Compare monthly sales by region over time");
        assert_eq!(assessment.tier, RiskTier::Elevated);
        assert_eq!(assessment.route, AnalysisRoute::ModelGuided);
        assert!(assessment.signals.contains(&"comparison".into()));
        assert!(assessment.signals.contains(&"time_series".into()));
    }

    #[test]
    fn causal_and_predictive_questions_are_high_risk() {
        let assessment = assess("Why did sales decline and can we forecast next month?");
        assert_eq!(assessment.tier, RiskTier::High);
        assert_eq!(assessment.route, AnalysisRoute::ModelGuided);
        assert!(assessment.signals.contains(&"explanation".into()));
        assert!(assessment.signals.contains(&"forecast".into()));
    }

    #[test]
    fn signed_financial_questions_raise_attention_signals() {
        let assessment = assess("What was net spending, excluding income and including refunds?");
        assert_eq!(assessment.tier, RiskTier::Elevated);
        assert_eq!(assessment.route, AnalysisRoute::ModelGuided);
        assert!(assessment.signals.contains(&"signed_values".into()));
    }

    #[test]
    fn quantity_comparison_is_not_misclassified_as_grouping() {
        let assessment =
            assess("Did I pay more in the second period or first period, and by how much?");
        assert_eq!(assessment.tier, RiskTier::Low);
        assert!(!assessment.signals.contains(&"grouping".into()));

        let grouped = assess("Break revenue down by region");
        assert_eq!(grouped.tier, RiskTier::Elevated);
        assert!(grouped.signals.contains(&"grouping".into()));
    }
}
