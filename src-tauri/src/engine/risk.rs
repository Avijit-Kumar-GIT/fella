//! Deterministic routing signals for an analytical question.
//!
//! This is deliberately a risk gate, not a semantic interpreter. The model
//! still proposes the meaning of a question; the runtime only decides when a
//! compact contract should be requested before data tools are used.

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
    FastPath,
    ContractFirst,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub tier: RiskTier,
    pub route: AnalysisRoute,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<String>,
}

impl RiskAssessment {
    pub fn requires_contract(&self) -> bool {
        matches!(self.route, AnalysisRoute::ContractFirst)
    }
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
            has_any_word(
                &words,
                &["group", "breakdown", "per", "each", "across", "by"],
            ),
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
    let route = if matches!(tier, RiskTier::Low) {
        AnalysisRoute::FastPath
    } else {
        AnalysisRoute::ContractFirst
    };
    RiskAssessment {
        tier,
        route,
        signals,
    }
}

fn has_word(words: &[&str], expected: &str) -> bool {
    words.iter().any(|word| *word == expected)
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
    fn obvious_lookups_stay_on_the_fast_path() {
        let assessment = assess("How many rows are in sales?");
        assert_eq!(assessment.tier, RiskTier::Low);
        assert!(!assessment.requires_contract());
    }

    #[test]
    fn comparisons_and_time_series_get_a_contract() {
        let assessment = assess("Compare monthly sales by region over time");
        assert_eq!(assessment.tier, RiskTier::Elevated);
        assert!(assessment.requires_contract());
        assert!(assessment.signals.contains(&"comparison".into()));
        assert!(assessment.signals.contains(&"time_series".into()));
    }

    #[test]
    fn causal_and_predictive_questions_are_high_risk() {
        let assessment = assess("Why did sales decline and can we forecast next month?");
        assert_eq!(assessment.tier, RiskTier::High);
        assert!(assessment.requires_contract());
        assert!(assessment.signals.contains(&"explanation".into()));
        assert!(assessment.signals.contains(&"forecast".into()));
    }
}
