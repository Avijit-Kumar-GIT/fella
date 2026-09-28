//! Versioned, auditable semantic facts for one mounted workspace.
//!
//! `memory.md` remains the human-readable surface. The typed fact ledger next
//! to it is the machine-readable record used to carry authority, revision,
//! provenance, and conflict state without making Markdown carry hidden
//! semantics. Facts are deliberately small and local; this is not a vector
//! database or a remote memory service.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_FACTS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Vocabulary,
    Definition,
    FieldBinding,
    Preference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactAuthority {
    User,
    Observed,
    ModelSuggestion,
    Confirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactState {
    Active,
    Proposed,
    Superseded,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticFact {
    pub id: String,
    pub kind: FactKind,
    pub key: String,
    pub statement: String,
    pub authority: FactAuthority,
    pub state: FactState,
    /// Canonical workspace path. Facts never cross this boundary.
    pub workspace: String,
    /// The catalog revision against which this fact was observed or confirmed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supporting_turn: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_ids: Vec<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts_with: Vec<String>,
}

impl SemanticFact {
    pub fn new(
        kind: FactKind,
        key: impl Into<String>,
        statement: impl Into<String>,
        authority: FactAuthority,
        workspace: impl Into<String>,
        revision: Option<String>,
        supporting_turn: Option<String>,
        evidence_ids: Vec<String>,
        at_ms: u64,
    ) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let id = format!("fact-{at_ms:x}-{sequence:x}");
        Self {
            id,
            kind,
            key: key.into().trim().to_string(),
            statement: statement.into().trim().to_string(),
            authority,
            state: if authority == FactAuthority::ModelSuggestion {
                FactState::Proposed
            } else {
                FactState::Active
            },
            workspace: workspace.into(),
            revision,
            supporting_turn,
            evidence_ids,
            created_at_ms: at_ms,
            updated_at_ms: at_ms,
            supersedes: Vec::new(),
            conflicts_with: Vec::new(),
        }
    }

    pub fn is_promptable(&self, current_revision: Option<&str>) -> bool {
        if self.state != FactState::Active || self.authority == FactAuthority::ModelSuggestion {
            return false;
        }
        match self.authority {
            FactAuthority::Observed => self.revision.as_deref() == current_revision,
            FactAuthority::User | FactAuthority::Confirmed => true,
            FactAuthority::ModelSuggestion => false,
        }
    }

    pub fn is_stale(&self, current_revision: Option<&str>) -> bool {
        self.revision.is_some() && self.revision.as_deref() != current_revision
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Added,
    Refreshed,
    SupersededExisting,
    Conflict,
}

#[derive(Default)]
pub struct SemanticMemory {
    path: PathBuf,
    facts: Vec<SemanticFact>,
}

/// The typed ledger lives beside `memory.md` and follows the same workspace
/// key. It is intentionally a simple JSONL file so corruption is localized to
/// individual facts and the user can inspect it without a database tool.
pub fn facts_path_for(memory_path: &Path) -> PathBuf {
    memory_path.with_extension("facts.jsonl")
}

impl SemanticMemory {
    pub fn load(path: &Path) -> Self {
        let mut memory = Self {
            path: path.to_path_buf(),
            facts: Vec::new(),
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return memory;
        };
        for line in text.lines() {
            let Ok(fact) = serde_json::from_str::<SemanticFact>(line) else {
                log::warn!(
                    "semantic memory: ignoring malformed fact in {}",
                    path.display()
                );
                continue;
            };
            if !fact.key.is_empty() && !fact.statement.is_empty() {
                memory.facts.push(fact);
            }
        }
        if memory.facts.len() > MAX_FACTS {
            let trim = memory.facts.len() - MAX_FACTS;
            memory.facts.drain(0..trim);
        }
        memory
    }

    pub fn facts(&self) -> &[SemanticFact] {
        &self.facts
    }

    pub fn has_kind(&self, kind: FactKind) -> bool {
        self.facts.iter().any(|fact| fact.kind == kind)
    }

    /// Add or reconcile one fact. Stronger user/confirmed facts supersede
    /// older definitions; conflicting observations are retained as conflicts
    /// rather than silently choosing a winner.
    pub fn upsert(&mut self, mut incoming: SemanticFact) -> UpsertOutcome {
        let same = self.facts.iter().position(|fact| {
            fact.kind == incoming.kind
                && fact.key.eq_ignore_ascii_case(&incoming.key)
                && normalize(&fact.statement) == normalize(&incoming.statement)
                && fact.state == FactState::Active
        });
        if let Some(index) = same {
            let existing = &mut self.facts[index];
            if authority_rank(incoming.authority) >= authority_rank(existing.authority) {
                existing.authority = incoming.authority;
                existing.revision = incoming
                    .revision
                    .take()
                    .or_else(|| existing.revision.clone());
                existing.supporting_turn = incoming
                    .supporting_turn
                    .take()
                    .or_else(|| existing.supporting_turn.clone());
                existing.evidence_ids = merge_ids(&existing.evidence_ids, &incoming.evidence_ids);
                existing.updated_at_ms = incoming.updated_at_ms;
            }
            return UpsertOutcome::Refreshed;
        }

        let conflicting: Vec<String> = self
            .facts
            .iter()
            .filter(|fact| {
                fact.kind == incoming.kind
                    && fact.key.eq_ignore_ascii_case(&incoming.key)
                    && matches!(fact.state, FactState::Active | FactState::Conflict)
                    && normalize(&fact.statement) != normalize(&incoming.statement)
            })
            .map(|fact| fact.id.clone())
            .collect();

        if incoming.authority == FactAuthority::ModelSuggestion {
            incoming.state = FactState::Proposed;
            self.push(incoming);
            return UpsertOutcome::Added;
        }

        match incoming.authority {
            FactAuthority::User | FactAuthority::Confirmed => {
                if !conflicting.is_empty() {
                    for fact in &mut self.facts {
                        if conflicting.iter().any(|id| id == &fact.id) {
                            fact.state = FactState::Superseded;
                            fact.updated_at_ms = incoming.updated_at_ms;
                        }
                    }
                    incoming.supersedes = conflicting;
                    self.push(incoming);
                    UpsertOutcome::SupersededExisting
                } else {
                    self.push(incoming);
                    UpsertOutcome::Added
                }
            }
            FactAuthority::Observed => {
                if conflicting.is_empty() {
                    self.push(incoming);
                    UpsertOutcome::Added
                } else {
                    let stronger: Vec<String> = self
                        .facts
                        .iter()
                        .filter(|fact| {
                            conflicting.iter().any(|id| id == &fact.id)
                                && matches!(
                                    fact.authority,
                                    FactAuthority::User | FactAuthority::Confirmed
                                )
                        })
                        .map(|fact| fact.id.clone())
                        .collect();
                    if !stronger.is_empty() {
                        incoming.state = FactState::Conflict;
                        incoming.conflicts_with = stronger;
                        self.push(incoming);
                        return UpsertOutcome::Conflict;
                    }
                    for fact in &mut self.facts {
                        if conflicting.iter().any(|id| id == &fact.id) {
                            fact.state = FactState::Conflict;
                            fact.conflicts_with.push(incoming.id.clone());
                            fact.conflicts_with.sort();
                            fact.conflicts_with.dedup();
                            fact.updated_at_ms = incoming.updated_at_ms;
                        }
                    }
                    incoming.state = FactState::Conflict;
                    incoming.conflicts_with = conflicting;
                    self.push(incoming);
                    UpsertOutcome::Conflict
                }
            }
            FactAuthority::ModelSuggestion => unreachable!(),
        }
    }

    fn push(&mut self, fact: SemanticFact) {
        self.facts.push(fact);
        if self.facts.len() > MAX_FACTS {
            let trim = self.facts.len() - MAX_FACTS;
            self.facts.drain(0..trim);
        }
    }

    pub fn save(&self) {
        if self.facts.is_empty() {
            let _ = std::fs::remove_file(&self.path);
            return;
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let Ok(mut file) = std::fs::File::create(&self.path) else {
            log::warn!("semantic memory: cannot write {}", self.path.display());
            return;
        };
        for fact in &self.facts {
            match serde_json::to_string(fact) {
                Ok(line) => {
                    if let Err(error) = writeln!(file, "{line}") {
                        log::warn!("semantic memory: write {}: {error}", self.path.display());
                        return;
                    }
                }
                Err(error) => log::warn!("semantic memory: serialize fact: {error}"),
            }
        }
    }

    /// A compact prompt projection. User/confirmed facts remain available
    /// across revisions, while observed facts only become guidance when they
    /// match the current catalog revision. Stale observations and conflicts
    /// are surfaced explicitly so the model cannot silently select one.
    pub fn prompt_block(&self, current_revision: Option<&str>) -> Option<String> {
        let mut trusted = Vec::new();
        let mut review = Vec::new();
        for fact in &self.facts {
            if fact.is_promptable(current_revision) {
                let age = if fact.is_stale(current_revision) {
                    ", prior revision"
                } else {
                    ""
                };
                trusted.push(format!(
                    "- {}: {} [{}{}]",
                    fact.key,
                    fact.statement,
                    authority_label(fact.authority),
                    age
                ));
            } else if fact.state == FactState::Conflict
                || (fact.authority == FactAuthority::Observed && fact.is_stale(current_revision))
            {
                review.push(format!(
                    "- {}: {} [{}; re-check before using]",
                    fact.key,
                    fact.statement,
                    if fact.state == FactState::Conflict {
                        "conflict"
                    } else {
                        "stale observation"
                    }
                ));
            }
        }
        if trusted.is_empty() && review.is_empty() {
            return None;
        }
        let mut block = String::from(
            "Versioned semantic facts for this folder (reference only; the runtime still checks them against the current data):\n",
        );
        if !trusted.is_empty() {
            block.push_str(&trusted.join("\n"));
        }
        if !review.is_empty() {
            if !trusted.is_empty() {
                block.push('\n');
            }
            block.push_str(
                "Definitions requiring review; do not silently choose between conflicting meanings:\n",
            );
            block.push_str(&review.join("\n"));
        }
        Some(block)
    }

    /// Render the user-visible projection that is appended to `memory.md`.
    /// It includes metadata needed for auditing without requiring a JSON
    /// reader, while the JSONL file remains the lossless representation.
    pub fn markdown_projection(&self) -> String {
        let mut out = String::from(
            "## Semantic facts\n\n<!-- Generated from the adjacent .facts.jsonl ledger. Edit the vocabulary/table notes above or delete this section; Fella never treats a model suggestion as a durable fact. -->\n",
        );
        for fact in &self.facts {
            let mut metadata = format!(
                "{}; {}; state={}; revision={}",
                authority_label(fact.authority),
                kind_label(fact.kind),
                state_label(fact.state),
                fact.revision.as_deref().unwrap_or("unknown")
            );
            if let Some(turn) = &fact.supporting_turn {
                metadata.push_str(&format!("; turn={turn}"));
            }
            if !fact.evidence_ids.is_empty() {
                metadata.push_str(&format!("; evidence={}", fact.evidence_ids.join(",")));
            }
            out.push_str(&format!(
                "- `{}` — {} [{}]\n",
                fact.key,
                one_line(&fact.statement),
                metadata
            ));
        }
        out
    }
}

/// Replace the generated `## Semantic facts` section without disturbing the
/// user-authored notes or unknown sections in `memory.md`.
pub fn sync_markdown_projection(memory_path: &Path, memory: &SemanticMemory) {
    if memory.facts.is_empty() {
        return;
    }
    let existing = std::fs::read_to_string(memory_path).unwrap_or_default();
    let mut kept = Vec::new();
    let mut in_generated = false;
    for line in existing.lines() {
        if line.trim().eq_ignore_ascii_case("## Semantic facts") {
            in_generated = true;
            continue;
        }
        if in_generated && line.starts_with("## ") {
            in_generated = false;
        }
        if !in_generated {
            kept.push(line);
        }
    }
    let mut output = kept.join("\n");
    if !output.trim().is_empty() {
        output.push_str("\n\n");
    }
    output.push_str(&memory.markdown_projection());
    if let Err(error) = std::fs::write(memory_path, output + "\n") {
        log::warn!(
            "semantic memory: projection {}: {error}",
            memory_path.display()
        );
    }
}

fn authority_rank(authority: FactAuthority) -> u8 {
    match authority {
        FactAuthority::ModelSuggestion => 1,
        FactAuthority::Observed => 2,
        FactAuthority::User => 3,
        FactAuthority::Confirmed => 4,
    }
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn merge_ids(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut ids = existing.to_vec();
    ids.extend(incoming.iter().cloned());
    ids.sort();
    ids.dedup();
    ids
}

fn authority_label(authority: FactAuthority) -> &'static str {
    match authority {
        FactAuthority::User => "user",
        FactAuthority::Observed => "observed",
        FactAuthority::ModelSuggestion => "model suggestion",
        FactAuthority::Confirmed => "confirmed",
    }
}

fn kind_label(kind: FactKind) -> &'static str {
    match kind {
        FactKind::Vocabulary => "vocabulary",
        FactKind::Definition => "definition",
        FactKind::FieldBinding => "field binding",
        FactKind::Preference => "preference",
    }
}

fn state_label(state: FactState) -> &'static str {
    match state {
        FactState::Active => "active",
        FactState::Proposed => "proposed",
        FactState::Superseded => "superseded",
        FactState::Conflict => "conflict",
    }
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(
        kind: FactKind,
        key: &str,
        statement: &str,
        authority: FactAuthority,
        revision: &str,
        at_ms: u64,
    ) -> SemanticFact {
        SemanticFact::new(
            kind,
            key,
            statement,
            authority,
            "/workspace",
            Some(revision.into()),
            Some(format!("turn-{at_ms}")),
            vec![format!("e-{at_ms}")],
            at_ms,
        )
    }

    #[test]
    fn user_fact_supersedes_an_older_definition_and_keeps_history() {
        let mut memory = SemanticMemory::default();
        assert_eq!(
            memory.upsert(fact(
                FactKind::Vocabulary,
                "rent",
                "category = 'housing'",
                FactAuthority::User,
                "r1",
                1,
            )),
            UpsertOutcome::Added
        );
        assert_eq!(
            memory.upsert(fact(
                FactKind::Vocabulary,
                "rent",
                "category = 'rent'",
                FactAuthority::User,
                "r2",
                2,
            )),
            UpsertOutcome::SupersededExisting
        );
        assert_eq!(memory.facts.len(), 2);
        assert_eq!(memory.facts[0].state, FactState::Superseded);
        assert!(memory.facts[1].is_promptable(Some("r2")));
        assert!(memory
            .prompt_block(Some("r2"))
            .unwrap()
            .contains("category = 'rent'"));
    }

    #[test]
    fn observations_from_different_revisions_become_a_visible_conflict() {
        let mut memory = SemanticMemory::default();
        memory.upsert(fact(
            FactKind::FieldBinding,
            "revenue",
            "uses field amount",
            FactAuthority::Observed,
            "r1",
            1,
        ));
        assert_eq!(
            memory.upsert(fact(
                FactKind::FieldBinding,
                "revenue",
                "uses field gross",
                FactAuthority::Observed,
                "r2",
                2,
            )),
            UpsertOutcome::Conflict
        );
        assert!(memory
            .facts
            .iter()
            .all(|fact| fact.state == FactState::Conflict));
        let prompt = memory.prompt_block(Some("r2")).unwrap();
        assert!(prompt.contains("re-check before using"));
        assert!(!memory.facts[1].is_promptable(Some("r2")));
    }

    #[test]
    fn an_observation_cannot_demote_a_user_definition() {
        let mut memory = SemanticMemory::default();
        memory.upsert(fact(
            FactKind::Vocabulary,
            "rent",
            "category = 'housing'",
            FactAuthority::User,
            "r1",
            1,
        ));
        memory.upsert(fact(
            FactKind::Vocabulary,
            "rent",
            "category = 'lease'",
            FactAuthority::Observed,
            "r2",
            2,
        ));
        assert_eq!(memory.facts[0].state, FactState::Active);
        assert!(memory.facts[0].is_promptable(Some("r2")));
        assert_eq!(memory.facts[1].state, FactState::Conflict);
    }

    #[test]
    fn model_suggestions_are_stored_but_never_promptable() {
        let mut memory = SemanticMemory::default();
        memory.upsert(fact(
            FactKind::Definition,
            "revenue",
            "might mean gross amount",
            FactAuthority::ModelSuggestion,
            "r1",
            1,
        ));
        assert_eq!(memory.facts[0].state, FactState::Proposed);
        assert!(memory.prompt_block(Some("r1")).is_none());
        assert!(memory.markdown_projection().contains("model suggestion"));
    }

    #[test]
    fn projection_replaces_only_its_own_section() {
        let dir =
            std::env::temp_dir().join(format!("fella-semantic-memory-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("memory.md");
        std::fs::write(
            &path,
            "# notes\n\n## Notes\nkeep this\n\n## Semantic facts\nold generated line\n\n## Custom\nkeep custom\n",
        )
        .unwrap();
        let mut memory = SemanticMemory {
            path: facts_path_for(&path),
            facts: Vec::new(),
        };
        memory.upsert(fact(
            FactKind::Preference,
            "currency",
            "amounts are GBP",
            FactAuthority::User,
            "r1",
            1,
        ));
        sync_markdown_projection(&path, &memory);
        let output = std::fs::read_to_string(&path).unwrap();
        assert!(output.contains("keep this"));
        assert!(output.contains("keep custom"));
        assert!(!output.contains("old generated line"));
        assert!(output.contains("amounts are GBP"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
