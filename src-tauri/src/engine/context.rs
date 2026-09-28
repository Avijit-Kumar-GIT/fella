//! Bounded assembly of the context sent to the analytical model.
//!
//! The engine has several legitimate context sources: the current workspace
//! schema, user-authored definitions, confirmed folder memory, and the
//! current conversation. They should not be concatenated without a policy.
//! This module gives those sources one typed assembly point and applies
//! deterministic budgets only when a section is too large.

use std::collections::HashSet;

const TRUNCATION_NOTICE: &str =
    "\n[Additional context omitted; use the available tools to inspect it.]\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextSection {
    UserContext,
    WorkspaceSchema,
    Conversation,
    FolderMemory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextOmission {
    pub section: ContextSection,
    pub original_chars: usize,
    pub retained_chars: usize,
}

/// The prompt-ready context packet for one question. The packet is deliberately
/// a projection rather than a second source of truth: it contains bounded
/// views of the workspace model, user definitions, memory, and conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextPacket {
    pub user_context: Vec<String>,
    pub schema: String,
    pub recent: Option<String>,
    pub learned: Option<String>,
    pub omissions: Vec<ContextOmission>,
}

impl ContextPacket {
    pub fn was_truncated(&self) -> bool {
        !self.omissions.is_empty()
    }
}

/// Deterministic budgets for the model-facing context. They are intentionally
/// generous for ordinary folders; the assembler is a guardrail for an
/// unusually long `fella.md`, memory file, transcript, or schema rather than
/// a reason to trim normal analytical context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextAssembler {
    pub user_context_chars: usize,
    pub schema_chars: usize,
    pub conversation_chars: usize,
    pub memory_chars: usize,
}

impl Default for ContextAssembler {
    fn default() -> Self {
        Self {
            user_context_chars: 8_000,
            schema_chars: 24_000,
            conversation_chars: 8_000,
            memory_chars: 8_000,
        }
    }
}

impl ContextAssembler {
    /// Assemble the context for one question. Inputs are kept byte-for-byte
    /// identical when they fit their section budget, preserving the existing
    /// prompt behavior for ordinary workspaces.
    pub fn assemble(
        &self,
        question: &str,
        user_context: &[String],
        schema: &str,
        recent: Option<&str>,
        learned: Option<&str>,
    ) -> ContextPacket {
        let mut packet = ContextPacket {
            schema: String::new(),
            ..ContextPacket::default()
        };

        let mut remaining = self.user_context_chars;
        for context in user_context {
            if remaining == 0 {
                packet.omissions.push(ContextOmission {
                    section: ContextSection::UserContext,
                    original_chars: char_len(context),
                    retained_chars: 0,
                });
                continue;
            }
            let (bounded, truncated) =
                bounded_text(context, question, remaining, Retention::Relevant);
            let retained_chars = char_len(&bounded);
            remaining = remaining.saturating_sub(retained_chars);
            if truncated {
                packet.omissions.push(ContextOmission {
                    section: ContextSection::UserContext,
                    original_chars: char_len(context),
                    retained_chars,
                });
            }
            if !bounded.is_empty() {
                packet.user_context.push(bounded);
            }
        }

        let original_schema_chars = char_len(schema);
        let (schema, truncated) =
            bounded_text(schema, question, self.schema_chars, Retention::Relevant);
        if truncated {
            packet.omissions.push(ContextOmission {
                section: ContextSection::WorkspaceSchema,
                original_chars: original_schema_chars,
                retained_chars: char_len(&schema),
            });
        }
        packet.schema = schema;

        if let Some(recent) = recent.filter(|text| !text.trim().is_empty()) {
            let (bounded, truncated) =
                bounded_text(recent, question, self.conversation_chars, Retention::Recent);
            if truncated {
                packet.omissions.push(ContextOmission {
                    section: ContextSection::Conversation,
                    original_chars: char_len(recent),
                    retained_chars: char_len(&bounded),
                });
            }
            packet.recent = (!bounded.is_empty()).then_some(bounded);
        }

        if let Some(learned) = learned.filter(|text| !text.trim().is_empty()) {
            let (bounded, truncated) =
                bounded_text(learned, question, self.memory_chars, Retention::Relevant);
            if truncated {
                packet.omissions.push(ContextOmission {
                    section: ContextSection::FolderMemory,
                    original_chars: char_len(learned),
                    retained_chars: char_len(&bounded),
                });
            }
            packet.learned = (!bounded.is_empty()).then_some(bounded);
        }

        packet
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Retention {
    Relevant,
    Recent,
}

fn char_len(value: &str) -> usize {
    value.chars().count()
}

fn bounded_text(text: &str, question: &str, budget: usize, retention: Retention) -> (String, bool) {
    let original_chars = char_len(text);
    if original_chars <= budget {
        return (text.to_string(), false);
    }
    if budget == 0 {
        return (String::new(), true);
    }

    let notice_chars = char_len(TRUNCATION_NOTICE);
    let content_budget = budget.saturating_sub(notice_chars);
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() || content_budget == 0 {
        return (clip_chars(TRUNCATION_NOTICE, budget), true);
    }

    let selected = match retention {
        Retention::Recent => select_recent(&lines, content_budget),
        Retention::Relevant => select_relevant(&lines, question, content_budget),
    };
    let mut result = render_lines(&lines, &selected, content_budget);
    result.push_str(TRUNCATION_NOTICE);
    if char_len(&result) > budget {
        result = clip_chars(&result, budget);
    }
    debug_assert!(char_len(&result) <= budget);
    debug_assert!(char_len(&result) < original_chars || budget == 0);
    (result, true)
}

fn select_recent(lines: &[&str], budget: usize) -> Vec<usize> {
    let mut selected = Vec::new();
    let mut used = 0;
    // Keep the section heading when possible, then retain the newest lines.
    if let Some(first) = lines.first() {
        let cost = line_cost(first);
        if cost <= budget {
            selected.push(0);
            used += cost;
        }
    }
    for index in (1..lines.len()).rev() {
        let cost = line_cost(lines[index]);
        if used + cost > budget {
            continue;
        }
        selected.push(index);
        used += cost;
    }
    selected.sort_unstable();
    selected
}

fn select_relevant(lines: &[&str], question: &str, budget: usize) -> Vec<usize> {
    let terms = terms(question);
    let mut ranked: Vec<(usize, usize)> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| (index, line_score(line, index, &terms)))
        .collect();
    ranked.sort_by(|(left_index, left_score), (right_index, right_score)| {
        right_score
            .cmp(left_score)
            .then(left_index.cmp(right_index))
    });

    let mut selected = Vec::new();
    let mut used = 0;
    for (index, _score) in ranked {
        let cost = line_cost(lines[index]);
        if used + cost > budget {
            continue;
        }
        selected.push(index);
        used += cost;
    }

    // If no line matched the question, keep a deterministic prefix instead of
    // sending an empty semantic section to the model.
    if selected.is_empty() {
        for (index, line) in lines.iter().enumerate() {
            let cost = line_cost(line);
            if used + cost > budget {
                break;
            }
            selected.push(index);
            used += cost;
        }
    }
    selected.sort_unstable();
    selected
}

fn line_cost(line: &str) -> usize {
    char_len(line) + 1
}

fn render_lines(lines: &[&str], selected: &[usize], budget: usize) -> String {
    let mut result = String::new();
    for index in selected {
        let line = lines[*index];
        let remaining = budget.saturating_sub(char_len(&result));
        if remaining == 0 {
            break;
        }
        let rendered = clip_chars(&format!("{line}\n"), remaining);
        result.push_str(&rendered);
        if rendered.chars().count() < line_cost(line) {
            break;
        }
    }
    result
}

fn clip_chars(value: &str, budget: usize) -> String {
    value.chars().take(budget).collect()
}

fn terms(value: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    value
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .map(str::trim)
        .filter(|term| term.chars().count() >= 3)
        .map(str::to_lowercase)
        .filter(|term| seen.insert(term.clone()))
        .collect()
}

fn line_score(line: &str, index: usize, question_terms: &[String]) -> usize {
    let lower = line.to_lowercase();
    let overlap = question_terms
        .iter()
        .filter(|term| lower.contains(term.as_str()))
        .count();
    let structural = if index == 0
        || line.trim_start().starts_with('#')
        || (line.trim_end().ends_with(':') && line.trim().len() < 100)
    {
        2
    } else {
        0
    };
    overlap * 10 + structural
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_context_is_preserved_exactly() {
        let assembler = ContextAssembler::default();
        let user = vec!["Amounts are in GBP.".to_string()];
        let schema = "Tables:\n  ledger (4 rows)\n";
        let recent = "Earlier in this conversation:\n- Q: total? A: 10\n";
        let learned = "Learned notes for this folder:\n- Rent is monthly.\n";

        let packet = assembler.assemble(
            "what is the total?",
            &user,
            schema,
            Some(recent),
            Some(learned),
        );

        assert_eq!(packet.user_context, user);
        assert_eq!(packet.schema, schema);
        assert_eq!(packet.recent.as_deref(), Some(recent));
        assert_eq!(packet.learned.as_deref(), Some(learned));
        assert!(!packet.was_truncated());
    }

    #[test]
    fn oversized_relevant_context_keeps_matching_lines_and_reports_omission() {
        let assembler = ContextAssembler {
            user_context_chars: 180,
            schema_chars: 10_000,
            conversation_chars: 10_000,
            memory_chars: 10_000,
        };
        let user = vec![
            "# Workspace rules\nIgnore this unrelated section.\nUse the amount_paid field for rent totals.\nAnother unrelated note.\nA further unrelated note that should be dropped.\nOne more long note to make this section exceed its budget."
                .into(),
        ];
        let packet = assembler.assemble("what is rent amount?", &user, "schema", None, None);
        let context = &packet.user_context[0];

        assert!(context.contains("amount_paid"));
        assert!(context.contains("Additional context omitted"));
        assert!(packet
            .omissions
            .iter()
            .any(|item| item.section == ContextSection::UserContext));
        assert!(context.chars().count() <= assembler.user_context_chars);
    }

    #[test]
    fn oversized_conversation_keeps_the_heading_and_newest_lines() {
        let assembler = ContextAssembler {
            user_context_chars: 10_000,
            schema_chars: 10_000,
            conversation_chars: 180,
            memory_chars: 10_000,
        };
        let recent = "Earlier in this conversation:\n- Q: first question A: old\n- Q: second question A: middle\n- Q: third question A: older\n- Q: fourth question A: older still\n- Q: newest question A: current\n";
        let packet = assembler.assemble("newest", &[], "schema", Some(recent), None);
        let recent = packet.recent.unwrap();

        assert!(recent.contains("Earlier in this conversation"));
        assert!(recent.contains("newest question"));
        assert!(recent.contains("Additional context omitted"));
        assert!(recent.chars().count() <= assembler.conversation_chars);
    }
}
