//! Durable product record for one bounded unit of analyst work.
//!
//! The Investigation deliberately does not embed conversation messages or
//! analysis-turn payloads. Those records remain owned by their existing
//! stores and are linked in later product-model slices.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::engine::error::{EngineError, EngineResult};

const INVESTIGATIONS_DIR: &str = "investigations";
const QUESTIONS_DIR: &str = "questions";
const DEFINITIONS_DIR: &str = "definitions";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationStatus {
    Active,
    Complete,
    Archived,
}

/// The current, analyst-editable statement of intent for an Investigation.
/// The original request remains owned by its Conversation message; this value
/// is only a proposed working summary and can be changed independently.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationBrief {
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub audience: String,
    #[serde(default)]
    pub desired_output: String,
    #[serde(default)]
    pub constraints: Vec<String>,
}

/// A durable Question record. The root Question represents the Investigation's
/// main request; decomposed Questions point to it through `parent_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationQuestion {
    pub id: String,
    pub investigation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub text: String,
    pub position: usize,
    #[serde(default)]
    pub status: InvestigationQuestionStatus,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationQuestionStatus {
    #[default]
    Active,
    Removed,
}

/// Editable plan input. Position is the input order; the engine assigns the
/// durable record's position and lifecycle timestamps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationQuestionInput {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub text: String,
}

/// Analyst-authored meaning or explicit assumption used only inside one
/// Investigation. The current value is mutable; prior values remain in the
/// record history and each AnalysisTurn captures the exact revision it used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationDefinition {
    pub id: String,
    pub investigation_id: String,
    pub kind: InvestigationDefinitionKind,
    pub text: String,
    pub revision: u64,
    #[serde(default)]
    pub history: Vec<InvestigationDefinitionRevision>,
    #[serde(default)]
    pub status: InvestigationDefinitionStatus,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationDefinitionKind {
    #[default]
    Definition,
    Assumption,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationDefinitionStatus {
    #[default]
    Active,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationDefinitionRevision {
    pub revision: u64,
    pub kind: InvestigationDefinitionKind,
    pub text: String,
    pub recorded_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationDefinitionInput {
    pub id: String,
    pub kind: InvestigationDefinitionKind,
    pub text: String,
}

/// Immutable copy embedded in a canonical AnalysisTurn, so editing or removing
/// a definition cannot change the context of an older run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationDefinitionSnapshot {
    pub id: String,
    pub revision: u64,
    pub kind: InvestigationDefinitionKind,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationContextSnapshot {
    pub investigation_id: String,
    #[serde(default)]
    pub definitions: Vec<InvestigationDefinitionSnapshot>,
    /// The exact source selection used by this turn. Older turns had no
    /// selection and therefore retain the historical all-sources behavior.
    #[serde(default)]
    pub source_selection: InvestigationSourceSelection,
}

/// Analyst-selected input boundary for one Investigation. Paths are
/// workspace-relative so duplicate filenames remain distinct and snapshots
/// do not depend on the machine's absolute mount path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InvestigationSourceSelection {
    pub mode: InvestigationSourceSelectionMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_paths: Vec<String>,
}

impl Default for InvestigationSourceSelection {
    fn default() -> Self {
        Self {
            mode: InvestigationSourceSelectionMode::AllSources,
            included_paths: Vec::new(),
        }
    }
}

impl InvestigationSourceSelection {
    pub fn includes_path(&self, path: &str) -> bool {
        match self.mode {
            InvestigationSourceSelectionMode::AllSources => true,
            InvestigationSourceSelectionMode::SelectedSources => {
                self.included_paths.iter().any(|included| included == path)
            }
        }
    }

    pub fn is_all_sources(&self) -> bool {
        self.mode == InvestigationSourceSelectionMode::AllSources
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationSourceSelectionMode {
    #[default]
    AllSources,
    SelectedSources,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionBreakdownProposal {
    pub subquestions: Vec<String>,
}

/// Validate the model's response before presenting it as an editable draft.
/// Suggestions are never accepted or persisted by this parser.
pub fn parse_question_breakdown(
    response: &str,
    main_question: &str,
) -> EngineResult<QuestionBreakdownProposal> {
    let response = response.trim();
    let response = if let Some(fenced) = response.strip_prefix("```") {
        let content = fenced
            .find('\n')
            .map(|newline| &fenced[newline + 1..])
            .unwrap_or(fenced);
        content.strip_suffix("```").unwrap_or(content).trim()
    } else {
        response
    };
    let mut proposal: QuestionBreakdownProposal = serde_json::from_str(response)
        .map_err(|error| EngineError::msg(format!("could not read Question breakdown: {error}")))?;
    if !(2..=6).contains(&proposal.subquestions.len()) {
        return Err(EngineError::msg(
            "the suggested breakdown must contain between 2 and 6 subquestions",
        ));
    }

    let main_key = main_question.trim().to_lowercase();
    let mut seen = HashSet::new();
    let mut total_chars = 0usize;
    for question in &mut proposal.subquestions {
        *question = question.trim().to_owned();
        let key = question.to_lowercase();
        if question.is_empty()
            || question.chars().count() > 12_000
            || key == main_key
            || !seen.insert(key)
        {
            return Err(EngineError::msg(
                "the suggested breakdown contains a blank, repeated, or duplicate main Question",
            ));
        }
        total_chars = total_chars.saturating_add(question.chars().count());
    }
    if total_chars > 24_000 {
        return Err(EngineError::msg(
            "the suggested Question breakdown exceeds the 24000 character limit",
        ));
    }
    Ok(proposal)
}

/// Stable, independently loadable identity for an investigation.
///
/// `workspace_id` is the canonical local folder identity, matching the
/// desktop's existing workspace contract. Conversation and run associations
/// are IDs only; their records remain owned by the existing stores.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Investigation {
    pub id: String,
    pub workspace_id: String,
    /// Stable link to the canonical archived Conversation transcript.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Ordered plan value. `None` identifies a pre-plan legacy record;
    /// `Some(vec![])` is an intentionally empty plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub questions: Option<Vec<InvestigationQuestion>>,
    /// Active definitions in analyst display order. Their independent records
    /// and prior revisions live in `definitions/<id>.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definitions: Option<Vec<InvestigationDefinition>>,
    /// Current analyst-selected files. Each AnalysisTurn separately stores
    /// the immutable selection snapshot it ran with.
    #[serde(default)]
    pub source_selection: InvestigationSourceSelection,
    pub title: String,
    #[serde(default)]
    pub brief: InvestigationBrief,
    /// The Conversation message used to draft the brief. The message itself is
    /// not copied here and remains unchanged when the brief is edited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief_source_message_id: Option<String>,
    /// Latest conversation message the analyst explicitly accepted or
    /// dismissed as a possible brief change. This is a cursor, not a copy of
    /// the message, and prevents reviewed suggestions from resurfacing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief_reviewed_message_id: Option<String>,
    /// IDs of canonical AnalysisTurn records associated with this work. The
    /// run payload and Conversation messages remain owned by their stores.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub analysis_turn_ids: Vec<String>,
    pub status: InvestigationStatus,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

/// On-disk aggregate. Question payloads live in `questions/<id>.json`; the
/// aggregate retains their ordered IDs and a legacy inline field solely for
/// one-way migration of records written before Question records existed.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct InvestigationRecord {
    id: String,
    workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    conversation_id: Option<String>,
    #[serde(default)]
    question_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    questions: Option<Vec<LegacyQuestion>>,
    #[serde(default)]
    definition_ids: Option<Vec<String>>,
    #[serde(default)]
    source_selection: InvestigationSourceSelection,
    title: String,
    #[serde(default)]
    brief: InvestigationBrief,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    brief_source_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    brief_reviewed_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    analysis_turn_ids: Vec<String>,
    status: InvestigationStatus,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LegacyQuestion {
    id: String,
    text: String,
}

fn investigations_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(INVESTIGATIONS_DIR)
}

fn record_path(data_dir: &Path, id: &str) -> EngineResult<PathBuf> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(EngineError::msg("invalid investigation id"));
    }
    Ok(investigations_dir(data_dir).join(format!("{id}.json")))
}

fn question_path(data_dir: &Path, id: &str) -> EngineResult<PathBuf> {
    if id.is_empty()
        || id.chars().count() > 128
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(EngineError::msg("invalid investigation question id"));
    }
    Ok(data_dir.join(QUESTIONS_DIR).join(format!("{id}.json")))
}

fn definition_path(data_dir: &Path, id: &str) -> EngineResult<PathBuf> {
    if id.is_empty()
        || id.chars().count() > 128
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(EngineError::msg("invalid investigation definition id"));
    }
    Ok(data_dir.join(DEFINITIONS_DIR).join(format!("{id}.json")))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn new_question_id() -> EngineResult<String> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random)
        .map_err(|error| EngineError::msg(format!("create question id: {error}")))?;
    Ok(format!(
        "q_{}",
        random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn initial_questions(
    investigation_id: &str,
    request: &str,
    timestamp: u64,
) -> EngineResult<Vec<InvestigationQuestion>> {
    let text = request.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![InvestigationQuestion {
        id: new_question_id()?,
        investigation_id: investigation_id.to_owned(),
        parent_id: None,
        text: text.to_owned(),
        position: 0,
        status: InvestigationQuestionStatus::Active,
        created_at_ms: timestamp,
        updated_at_ms: timestamp,
    }])
}

fn read_raw_record(path: &Path, expected_id: &str) -> EngineResult<InvestigationRecord> {
    let body = std::fs::read_to_string(path)
        .map_err(|error| EngineError::io(format!("read {}", path.display()), error))?;
    let record: InvestigationRecord = serde_json::from_str(&body).map_err(|error| {
        EngineError::msg(format!(
            "investigation {expected_id} is not valid JSON: {error}"
        ))
    })?;
    if record.id != expected_id {
        return Err(EngineError::msg(format!(
            "investigation file {} contains a mismatched id",
            path.display()
        )));
    }
    Ok(record)
}

fn write_json_atomic(path: &Path, key: &str, body: &[u8]) -> EngineResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| EngineError::msg("record path has no parent directory"))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| EngineError::io(format!("create {}", parent.display()), error))?;
    let mut random = [0u8; 8];
    getrandom::fill(&mut random)
        .map_err(|error| EngineError::msg(format!("create {key} update id: {error}")))?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let temporary = path.with_file_name(format!(".{key}-{suffix}.tmp"));
    let write_result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| EngineError::io(format!("create {}", temporary.display()), error))?;
        file.write_all(body)
            .map_err(|error| EngineError::io(format!("write {}", temporary.display()), error))?;
        file.sync_all()
            .map_err(|error| EngineError::io(format!("sync {}", temporary.display()), error))?;
        #[cfg(windows)]
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|error| EngineError::io(format!("replace {}", path.display()), error))?;
        }
        std::fs::rename(&temporary, path)
            .map_err(|error| EngineError::io(format!("replace {}", path.display()), error))
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    write_result
}

fn write_question_record(data_dir: &Path, question: &InvestigationQuestion) -> EngineResult<()> {
    let path = question_path(data_dir, &question.id)?;
    let body = serde_json::to_vec_pretty(question).map_err(EngineError::from)?;
    write_json_atomic(&path, &question.id, &body)
}

pub fn question_load(data_dir: &Path, id: &str) -> EngineResult<InvestigationQuestion> {
    let path = question_path(data_dir, id)?;
    let body = std::fs::read_to_string(&path)
        .map_err(|error| EngineError::io(format!("read {}", path.display()), error))?;
    let question: InvestigationQuestion = serde_json::from_str(&body)
        .map_err(|error| EngineError::msg(format!("question {id} is not valid JSON: {error}")))?;
    if question.id != id {
        return Err(EngineError::msg(format!(
            "question file {} contains a mismatched id",
            path.display()
        )));
    }
    Ok(question)
}

fn write_definition_record(
    data_dir: &Path,
    definition: &InvestigationDefinition,
) -> EngineResult<()> {
    let path = definition_path(data_dir, &definition.id)?;
    let body = serde_json::to_vec_pretty(definition).map_err(EngineError::from)?;
    write_json_atomic(&path, &definition.id, &body)
}

pub fn definition_load(data_dir: &Path, id: &str) -> EngineResult<InvestigationDefinition> {
    let path = definition_path(data_dir, id)?;
    let body = std::fs::read_to_string(&path)
        .map_err(|error| EngineError::io(format!("read {}", path.display()), error))?;
    let definition: InvestigationDefinition = serde_json::from_str(&body)
        .map_err(|error| EngineError::msg(format!("definition {id} is not valid JSON: {error}")))?;
    if definition.id != id {
        return Err(EngineError::msg(format!(
            "definition file {} contains a mismatched id",
            path.display()
        )));
    }
    Ok(definition)
}

/// Capture the active definition versions from the canonical Investigation.
/// The returned value is self-contained and suitable for an AnalysisTurn.
pub fn context_snapshot(data_dir: &Path, id: &str) -> EngineResult<InvestigationContextSnapshot> {
    let investigation = load(data_dir, id)?;
    let source_selection = investigation.source_selection.clone();
    let definitions = investigation
        .definitions
        .unwrap_or_default()
        .into_iter()
        .map(|definition| InvestigationDefinitionSnapshot {
            id: definition.id,
            revision: definition.revision,
            kind: definition.kind,
            text: definition.text,
        })
        .collect();
    Ok(InvestigationContextSnapshot {
        investigation_id: investigation.id,
        definitions,
        source_selection,
    })
}

/// Add only analyst-saved definitions to the prompt. They inform interpretation
/// but are not evidence and never replace the immediate user request.
pub fn question_with_context(question: &str, context: &InvestigationContextSnapshot) -> String {
    if context.definitions.is_empty() {
        return question.to_owned();
    }
    let definitions = context
        .definitions
        .iter()
        .map(|definition| {
            let kind = match definition.kind {
                InvestigationDefinitionKind::Definition => "Definition",
                InvestigationDefinitionKind::Assumption => "Assumption",
            };
            format!(
                "- {kind} (revision {}): {}",
                definition.revision, definition.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Saved Investigation context (use these meanings when interpreting the request; they are not evidence):\n{definitions}\n\nCurrent analyst request and plan:\n{question}"
    )
}

fn public_record(
    record: InvestigationRecord,
    questions: Vec<InvestigationQuestion>,
    definitions: Vec<InvestigationDefinition>,
) -> Investigation {
    Investigation {
        id: record.id,
        workspace_id: record.workspace_id,
        conversation_id: record.conversation_id,
        questions: Some(questions),
        definitions: Some(definitions),
        source_selection: record.source_selection,
        title: record.title,
        brief: record.brief,
        brief_source_message_id: record.brief_source_message_id,
        brief_reviewed_message_id: record.brief_reviewed_message_id,
        analysis_turn_ids: record.analysis_turn_ids,
        status: record.status,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    }
}

fn read_record(path: &Path, expected_id: &str, data_dir: &Path) -> EngineResult<Investigation> {
    let mut record = read_raw_record(path, expected_id)?;
    let mut changed = false;
    let definitions = if let Some(ids) = record.definition_ids.clone() {
        let mut loaded = Vec::with_capacity(ids.len());
        for id in ids {
            let definition = definition_load(data_dir, &id)?;
            if definition.investigation_id != expected_id
                || definition.status != InvestigationDefinitionStatus::Active
            {
                return Err(EngineError::msg(format!(
                    "definition {id} is not active in investigation {expected_id}"
                )));
            }
            loaded.push(definition);
        }
        loaded
    } else {
        record.definition_ids = Some(Vec::new());
        changed = true;
        Vec::new()
    };
    let questions = if let Some(ids) = record.question_ids.clone() {
        let mut loaded = Vec::with_capacity(ids.len());
        for id in ids {
            let question = question_load(data_dir, &id)?;
            if question.investigation_id != expected_id
                || question.status != InvestigationQuestionStatus::Active
            {
                return Err(EngineError::msg(format!(
                    "question {id} is not an active child of investigation {expected_id}"
                )));
            }
            loaded.push(question);
        }
        loaded.sort_by_key(|question| question.position);
        loaded
    } else {
        // Upgrade the former inline plan once. Its first item is the main
        // request; later items become children of that root Question.
        let legacy = match record.questions.take() {
            Some(questions) if !questions.is_empty() => questions,
            Some(_) if !record.brief.goal.trim().is_empty() => vec![LegacyQuestion {
                id: new_question_id()?,
                text: record.brief.goal.clone(),
            }],
            Some(questions) => questions,
            None if !record.brief.goal.trim().is_empty() => vec![LegacyQuestion {
                id: new_question_id()?,
                text: record.brief.goal.clone(),
            }],
            None => Vec::new(),
        };
        let root_id = legacy.first().map(|question| question.id.clone());
        let mut migrated = Vec::with_capacity(legacy.len());
        for (position, old) in legacy.into_iter().enumerate() {
            let id = old.id.trim().to_owned();
            let text = old.text.trim().to_owned();
            if id.is_empty() || text.is_empty() {
                return Err(EngineError::msg(format!(
                    "legacy question in investigation {expected_id} is blank"
                )));
            }
            let timestamp = record.created_at_ms;
            migrated.push(InvestigationQuestion {
                id,
                investigation_id: expected_id.to_owned(),
                parent_id: if position == 0 { None } else { root_id.clone() },
                text,
                position,
                status: InvestigationQuestionStatus::Active,
                created_at_ms: timestamp,
                updated_at_ms: timestamp,
            });
        }
        for question in &migrated {
            write_question_record(data_dir, question)?;
        }
        record.question_ids = Some(
            migrated
                .iter()
                .map(|question| question.id.clone())
                .collect(),
        );
        record.questions = None;
        changed = true;
        migrated
    };
    if changed {
        write_raw_record(data_dir, &record)?;
    }
    Ok(public_record(record, questions, definitions))
}

fn write_raw_record(data_dir: &Path, record: &InvestigationRecord) -> EngineResult<()> {
    let path = record_path(data_dir, &record.id)?;
    let body = serde_json::to_vec_pretty(record).map_err(EngineError::from)?;
    write_json_atomic(&path, &record.id, &body)
}

fn persist_new_record(data_dir: &Path, record: &Investigation) -> EngineResult<()> {
    let path = record_path(data_dir, &record.id)?;
    let stored = InvestigationRecord {
        id: record.id.clone(),
        workspace_id: record.workspace_id.clone(),
        conversation_id: record.conversation_id.clone(),
        question_ids: Some(
            record
                .questions
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|question| question.id.clone())
                .collect(),
        ),
        questions: None,
        definition_ids: Some(Vec::new()),
        source_selection: record.source_selection.clone(),
        title: record.title.clone(),
        brief: record.brief.clone(),
        brief_source_message_id: record.brief_source_message_id.clone(),
        brief_reviewed_message_id: record.brief_reviewed_message_id.clone(),
        analysis_turn_ids: record.analysis_turn_ids.clone(),
        status: record.status.clone(),
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    };
    let body = serde_json::to_vec_pretty(&stored).map_err(EngineError::from)?;
    let temporary = path.with_extension("json.tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| EngineError::io(format!("create {}", temporary.display()), error))?;
    if let Err(error) = file.write_all(&body) {
        let _ = std::fs::remove_file(&temporary);
        return Err(EngineError::io(
            format!("write {}", temporary.display()),
            error,
        ));
    }
    drop(file);
    match std::fs::hard_link(&temporary, &path) {
        Ok(()) => {
            let _ = std::fs::remove_file(&temporary);
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(EngineError::io(format!("write {}", path.display()), error))
        }
    }
}

pub fn create(
    data_dir: &Path,
    workspace_id: &str,
    title: &str,
    initial_request: Option<&str>,
    source_message_id: Option<&str>,
    conversation_id: Option<&str>,
) -> EngineResult<Investigation> {
    let workspace_id = workspace_id.trim();
    if workspace_id.is_empty() {
        return Err(EngineError::msg(
            "an investigation requires an explicit workspace id",
        ));
    }

    let title = title.trim();
    let title = if title.is_empty() {
        "New investigation"
    } else {
        title
    };
    let goal = initial_request.unwrap_or_default().trim();
    if goal.chars().count() > 12_000 {
        return Err(EngineError::msg(
            "initial request is too long for an investigation brief",
        ));
    }
    let conversation_id = conversation_id.map(str::trim).filter(|id| !id.is_empty());
    if conversation_id.is_some_and(|id| id.chars().count() > 256) {
        return Err(EngineError::msg("conversation id is too long"));
    }

    let directory = investigations_dir(data_dir);
    std::fs::create_dir_all(&directory)
        .map_err(|error| EngineError::io(format!("create {}", directory.display()), error))?;

    // Random IDs avoid coupling product identity to editable titles, paths,
    // ordering, or wall-clock precision. Retry only on the vanishingly rare
    // collision; never overwrite an existing record.
    for _ in 0..8 {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|error| EngineError::msg(format!("create investigation id: {error}")))?;
        let id = format!(
            "inv_{}",
            random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let path = record_path(data_dir, &id)?;
        if path.exists() {
            continue;
        }

        let timestamp = now_ms();
        let questions = initial_questions(&id, goal, timestamp)?;
        if questions
            .iter()
            .any(|question| question_path(data_dir, &question.id).is_ok_and(|path| path.exists()))
        {
            continue;
        }
        let record = Investigation {
            id: id.clone(),
            workspace_id: workspace_id.to_owned(),
            conversation_id: conversation_id.map(str::to_owned),
            questions: Some(questions.clone()),
            definitions: Some(Vec::new()),
            source_selection: InvestigationSourceSelection::default(),
            title: title.to_owned(),
            brief: InvestigationBrief {
                goal: goal.to_owned(),
                ..InvestigationBrief::default()
            },
            brief_source_message_id: if goal.is_empty() {
                None
            } else {
                source_message_id
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
            },
            brief_reviewed_message_id: if goal.is_empty() {
                None
            } else {
                source_message_id
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
            },
            analysis_turn_ids: Vec::new(),
            status: InvestigationStatus::Active,
            created_at_ms: timestamp,
            updated_at_ms: timestamp,
        };
        for question in &questions {
            write_question_record(data_dir, question)?;
        }
        persist_new_record(data_dir, &record)?;
        return Ok(record);
    }

    Err(EngineError::msg(
        "could not allocate a unique investigation id",
    ))
}

/// Resolve the Investigation already seeded by this conversation's first
/// user message, or create it without requiring a separate setup action.
pub fn ensure_for_conversation(
    data_dir: &Path,
    workspace_id: &str,
    conversation_id: &str,
    initial_request: &str,
    source_message_id: &str,
) -> EngineResult<Investigation> {
    const MAX_SOURCE_MESSAGE_ID_CHARS: usize = 256;

    let workspace_id = workspace_id.trim();
    let conversation_id = conversation_id.trim();
    let source_message_id = source_message_id.trim();
    if workspace_id.is_empty() {
        return Err(EngineError::msg(
            "an investigation requires an explicit workspace id",
        ));
    }
    if conversation_id.is_empty() || conversation_id.chars().count() > MAX_SOURCE_MESSAGE_ID_CHARS {
        return Err(EngineError::msg(
            "an investigation requires a valid conversation id",
        ));
    }
    if source_message_id.is_empty()
        || source_message_id.chars().count() > MAX_SOURCE_MESSAGE_ID_CHARS
    {
        return Err(EngineError::msg(
            "an investigation requires a valid source conversation message id",
        ));
    }

    let records = list(data_dir)?;
    let mut existing = records
        .iter()
        .find(|record| {
            record.workspace_id == workspace_id
                && record.conversation_id.as_deref() == Some(conversation_id)
        })
        .cloned()
        .or_else(|| {
            records.into_iter().find(|record| {
                record.workspace_id == workspace_id
                    && record.brief_source_message_id.as_deref() == Some(source_message_id)
            })
        });
    if let Some(mut existing) = existing.take() {
        let mut changed = false;
        if let Some(linked_id) = existing.conversation_id.as_deref() {
            if linked_id != conversation_id {
                return Err(EngineError::msg(
                    "the source message is already linked to another conversation",
                ));
            }
        } else {
            // Upgrade records created before Conversation IDs were explicit.
            existing.conversation_id = Some(conversation_id.to_owned());
            changed = true;
        }
        // A manually-created, blank Investigation can be attached to its
        // conversation before the analyst asks the first real question.
        if existing.brief_source_message_id.is_none() {
            let request = initial_request.trim();
            if !request.is_empty() {
                existing.brief.goal = request.to_owned();
                existing.brief_source_message_id = Some(source_message_id.to_owned());
                existing.brief_reviewed_message_id = Some(source_message_id.to_owned());
                if existing.questions.as_ref().is_some_and(Vec::is_empty) {
                    let timestamp = now_ms();
                    let questions = initial_questions(&existing.id, request, timestamp)?;
                    for question in &questions {
                        write_question_record(data_dir, question)?;
                    }
                    existing.questions = Some(questions);
                }
                changed = true;
            }
        }
        if changed {
            existing.updated_at_ms = now_ms().max(existing.updated_at_ms.saturating_add(1));
            write_record(data_dir, &existing)?;
        }
        return Ok(existing);
    }

    let title_text = initial_request
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let title: String = title_text.chars().take(64).collect();
    let title = if title_text.chars().count() > 64 {
        format!("{}…", title.chars().take(61).collect::<String>().trim_end())
    } else if title.is_empty() {
        "New investigation".to_owned()
    } else {
        title
    };
    create(
        data_dir,
        workspace_id,
        &title,
        Some(initial_request),
        Some(source_message_id),
        Some(conversation_id),
    )
}

/// Replace the current ordered Question plan. Prior AnalysisTurns are separate
/// immutable records and are intentionally not rewritten by this operation.
pub fn update_questions(
    data_dir: &Path,
    id: &str,
    mut questions: Vec<InvestigationQuestionInput>,
) -> EngineResult<Investigation> {
    const MAX_QUESTIONS: usize = 100;
    const MAX_ID_CHARS: usize = 128;
    const MAX_TEXT_CHARS: usize = 12_000;
    const MAX_PLAN_CHARS: usize = 24_000;

    if questions.len() > MAX_QUESTIONS {
        return Err(EngineError::msg(
            "an investigation plan has too many questions",
        ));
    }
    let mut ids = HashSet::new();
    let mut plan_chars = 0usize;
    for question in &mut questions {
        question.id = question.id.trim().to_owned();
        question.text = question.text.trim().to_owned();
        if question.id.is_empty()
            || question.id.chars().count() > MAX_ID_CHARS
            || !question.id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
            })
            || !ids.insert(question.id.clone())
        {
            return Err(EngineError::msg(
                "invalid or duplicate Investigation Question id",
            ));
        }
        if question.text.is_empty() || question.text.chars().count() > MAX_TEXT_CHARS {
            return Err(EngineError::msg(
                "Investigation Question text must be non-empty and no longer than 12000 characters",
            ));
        }
        plan_chars = plan_chars.saturating_add(question.text.chars().count());
        if plan_chars > MAX_PLAN_CHARS {
            return Err(EngineError::msg(
                "Investigation Question plan exceeds the 24000 character limit",
            ));
        }
    }

    if let Some(root) = questions.first() {
        if root.parent_id.is_some() {
            return Err(EngineError::msg(
                "the main Investigation Question cannot have a parent",
            ));
        }
        let root_id = root.id.clone();
        for child in questions.iter_mut().skip(1) {
            match child.parent_id.as_deref() {
                Some(parent_id) if parent_id != root_id => {
                    return Err(EngineError::msg(
                        "decomposed Questions must link to the main Investigation Question",
                    ));
                }
                None => child.parent_id = Some(root_id.clone()),
                _ => {}
            }
        }
    }

    let mut record = load(data_dir, id)?;
    let current = record.questions.clone().unwrap_or_default();
    let existing_timestamp = now_ms();
    let mut next = Vec::with_capacity(questions.len());
    for (position, input) in questions.into_iter().enumerate() {
        let path = question_path(data_dir, &input.id)?;
        let mut question = if path.exists() {
            let question = question_load(data_dir, &input.id)?;
            if question.investigation_id != record.id {
                return Err(EngineError::msg(format!(
                    "question {} belongs to another investigation",
                    input.id
                )));
            }
            question
        } else {
            InvestigationQuestion {
                id: input.id.clone(),
                investigation_id: record.id.clone(),
                parent_id: None,
                text: String::new(),
                position,
                status: InvestigationQuestionStatus::Active,
                created_at_ms: existing_timestamp,
                updated_at_ms: existing_timestamp,
            }
        };
        let changed = question.parent_id != input.parent_id
            || question.text != input.text
            || question.position != position
            || question.status != InvestigationQuestionStatus::Active;
        question.parent_id = input.parent_id;
        question.text = input.text;
        question.position = position;
        question.status = InvestigationQuestionStatus::Active;
        if changed || !path.exists() {
            question.updated_at_ms = now_ms().max(question.updated_at_ms.saturating_add(1));
            write_question_record(data_dir, &question)?;
        }
        next.push(question);
    }

    let next_ids: HashSet<_> = next.iter().map(|question| question.id.as_str()).collect();
    let changed = current != next;
    if changed {
        for mut question in current {
            if !next_ids.contains(question.id.as_str()) {
                question.status = InvestigationQuestionStatus::Removed;
                question.updated_at_ms = now_ms().max(question.updated_at_ms.saturating_add(1));
                write_question_record(data_dir, &question)?;
            }
        }
        record.questions = Some(next);
        record.updated_at_ms = now_ms().max(record.updated_at_ms.saturating_add(1));
        write_record(data_dir, &record)?;
    }
    Ok(record)
}

/// Replace the ordered active Definition/Assumption set. Content edits append
/// the previous value to each record's history; removing an entry changes its
/// lifecycle state instead of orphaning its identity.
pub fn update_definitions(
    data_dir: &Path,
    id: &str,
    mut inputs: Vec<InvestigationDefinitionInput>,
) -> EngineResult<Investigation> {
    const MAX_DEFINITIONS: usize = 100;
    const MAX_ID_CHARS: usize = 128;
    const MAX_TEXT_CHARS: usize = 4_000;
    const MAX_TOTAL_CHARS: usize = 24_000;

    if inputs.len() > MAX_DEFINITIONS {
        return Err(EngineError::msg(
            "an Investigation has too many saved definitions and assumptions",
        ));
    }
    let mut ids = HashSet::new();
    let mut total_chars = 0usize;
    for input in &mut inputs {
        input.id = input.id.trim().to_owned();
        input.text = input.text.trim().to_owned();
        if input.id.is_empty()
            || input.id.chars().count() > MAX_ID_CHARS
            || !input.id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
            })
            || !ids.insert(input.id.clone())
        {
            return Err(EngineError::msg(
                "invalid or duplicate Investigation definition id",
            ));
        }
        if input.text.is_empty() || input.text.chars().count() > MAX_TEXT_CHARS {
            return Err(EngineError::msg(
                "definition or assumption text must be non-empty and no longer than 4000 characters",
            ));
        }
        total_chars = total_chars.saturating_add(input.text.chars().count());
        if total_chars > MAX_TOTAL_CHARS {
            return Err(EngineError::msg(
                "saved definitions and assumptions exceed the 24000 character limit",
            ));
        }
    }

    let mut record = load(data_dir, id)?;
    let current = record.definitions.clone().unwrap_or_default();
    let timestamp = now_ms();
    let mut next = Vec::with_capacity(inputs.len());
    for input in inputs {
        let path = definition_path(data_dir, &input.id)?;
        let mut definition = if path.exists() {
            let definition = definition_load(data_dir, &input.id)?;
            if definition.investigation_id != record.id {
                return Err(EngineError::msg(format!(
                    "definition {} belongs to another Investigation",
                    input.id
                )));
            }
            definition
        } else {
            InvestigationDefinition {
                id: input.id.clone(),
                investigation_id: record.id.clone(),
                kind: input.kind,
                text: input.text.clone(),
                revision: 1,
                history: Vec::new(),
                status: InvestigationDefinitionStatus::Active,
                created_at_ms: timestamp,
                updated_at_ms: timestamp,
            }
        };

        let was_persisted = path.exists();
        let changed = !was_persisted
            || definition.kind != input.kind
            || definition.text != input.text
            || definition.status != InvestigationDefinitionStatus::Active;
        if changed {
            if was_persisted {
                definition.history.push(InvestigationDefinitionRevision {
                    revision: definition.revision,
                    kind: definition.kind,
                    text: definition.text.clone(),
                    recorded_at_ms: definition.updated_at_ms,
                });
                definition.revision = definition.revision.saturating_add(1);
                definition.kind = input.kind;
                definition.text = input.text;
                definition.status = InvestigationDefinitionStatus::Active;
                definition.updated_at_ms =
                    timestamp.max(definition.updated_at_ms.saturating_add(1));
            }
            write_definition_record(data_dir, &definition)?;
        }
        next.push(definition);
    }

    let next_ids: HashSet<_> = next.iter().map(|item| item.id.as_str()).collect();
    for mut definition in current {
        if !next_ids.contains(definition.id.as_str()) {
            definition.history.push(InvestigationDefinitionRevision {
                revision: definition.revision,
                kind: definition.kind,
                text: definition.text.clone(),
                recorded_at_ms: definition.updated_at_ms,
            });
            definition.revision = definition.revision.saturating_add(1);
            definition.status = InvestigationDefinitionStatus::Removed;
            definition.updated_at_ms = timestamp.max(definition.updated_at_ms.saturating_add(1));
            write_definition_record(data_dir, &definition)?;
        }
    }

    if record.definitions.as_ref() != Some(&next) {
        record.definitions = Some(next);
        record.updated_at_ms = now_ms().max(record.updated_at_ms.saturating_add(1));
        write_record(data_dir, &record)?;
    }
    Ok(record)
}

/// Replace the file selection used by subsequent runs. The selection is
/// stored on the Investigation; completed AnalysisTurns already contain an
/// immutable context snapshot and are therefore unaffected.
pub fn update_source_selection(
    data_dir: &Path,
    id: &str,
    mut selection: InvestigationSourceSelection,
) -> EngineResult<Investigation> {
    const MAX_SELECTED_PATHS: usize = 10_000;
    if selection.included_paths.len() > MAX_SELECTED_PATHS {
        return Err(EngineError::msg(
            "an Investigation cannot select more than 10000 source files",
        ));
    }
    let mut unique = HashSet::new();
    for path in &mut selection.included_paths {
        *path = normalize_workspace_relative_path(path)?;
        if !unique.insert(path.clone()) {
            return Err(EngineError::msg(
                "Investigation source paths must be unique",
            ));
        }
    }
    if selection.mode == InvestigationSourceSelectionMode::AllSources {
        selection.included_paths.clear();
    }

    let mut record = load(data_dir, id)?;
    if record.source_selection != selection {
        record.source_selection = selection;
        record.updated_at_ms = now_ms().max(record.updated_at_ms.saturating_add(1));
        write_record(data_dir, &record)?;
    }
    Ok(record)
}

fn normalize_workspace_relative_path(path: &str) -> EngineResult<String> {
    let path = path.trim().replace('\\', "/");
    let components: Vec<_> = path.split('/').collect();
    if path.is_empty()
        || path.starts_with('/')
        || path.contains(':')
        || components
            .iter()
            .any(|component| component.is_empty() || *component == "." || *component == "..")
    {
        return Err(EngineError::msg(
            "source selection paths must be normalized workspace-relative paths",
        ));
    }
    Ok(components.join("/"))
}

fn write_record(data_dir: &Path, record: &Investigation) -> EngineResult<()> {
    let stored = InvestigationRecord {
        id: record.id.clone(),
        workspace_id: record.workspace_id.clone(),
        conversation_id: record.conversation_id.clone(),
        question_ids: Some(
            record
                .questions
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter(|question| question.status == InvestigationQuestionStatus::Active)
                .map(|question| question.id.clone())
                .collect(),
        ),
        questions: None,
        definition_ids: Some(
            record
                .definitions
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter(|definition| definition.status == InvestigationDefinitionStatus::Active)
                .map(|definition| definition.id.clone())
                .collect(),
        ),
        source_selection: record.source_selection.clone(),
        title: record.title.clone(),
        brief: record.brief.clone(),
        brief_source_message_id: record.brief_source_message_id.clone(),
        brief_reviewed_message_id: record.brief_reviewed_message_id.clone(),
        analysis_turn_ids: record.analysis_turn_ids.clone(),
        status: record.status.clone(),
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    };
    write_raw_record(data_dir, &stored)
}

pub fn update_brief(
    data_dir: &Path,
    id: &str,
    mut brief: InvestigationBrief,
    reviewed_message_id: Option<&str>,
) -> EngineResult<Investigation> {
    const MAX_FIELD_CHARS: usize = 12_000;
    const MAX_CONSTRAINTS: usize = 40;
    const MAX_CONSTRAINT_CHARS: usize = 2_000;

    brief.goal = brief.goal.trim().to_owned();
    brief.audience = brief.audience.trim().to_owned();
    brief.desired_output = brief.desired_output.trim().to_owned();
    if brief.goal.chars().count() > MAX_FIELD_CHARS
        || brief.audience.chars().count() > MAX_FIELD_CHARS
        || brief.desired_output.chars().count() > MAX_FIELD_CHARS
    {
        return Err(EngineError::msg("investigation brief field is too long"));
    }
    if brief.constraints.len() > MAX_CONSTRAINTS {
        return Err(EngineError::msg(
            "investigation brief has too many constraints",
        ));
    }
    brief.constraints = brief
        .constraints
        .into_iter()
        .map(|constraint| constraint.trim().to_owned())
        .filter(|constraint| !constraint.is_empty())
        .collect();
    if brief
        .constraints
        .iter()
        .any(|constraint| constraint.chars().count() > MAX_CONSTRAINT_CHARS)
    {
        return Err(EngineError::msg("investigation constraint is too long"));
    }

    let mut record = load(data_dir, id)?;
    if let Some(message_id) = reviewed_message_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        if message_id.chars().count() > 256 {
            return Err(EngineError::msg(
                "reviewed conversation message id is too long",
            ));
        }
        record.brief_reviewed_message_id = Some(message_id.to_owned());
    }
    record.brief = brief;
    record.updated_at_ms = now_ms().max(record.updated_at_ms.saturating_add(1));
    write_record(data_dir, &record)?;
    Ok(record)
}

/// Append references to canonical run records without copying their payloads.
/// Duplicate links are ignored so conversation rehydration is idempotent.
pub fn link_analysis_turns(
    data_dir: &Path,
    id: &str,
    analysis_turn_ids: &[String],
) -> EngineResult<Investigation> {
    const MAX_LINKS_PER_UPDATE: usize = 500;
    const MAX_TURN_ID_CHARS: usize = 256;

    if analysis_turn_ids.len() > MAX_LINKS_PER_UPDATE {
        return Err(EngineError::msg("too many analysis runs to link at once"));
    }
    let mut record = load(data_dir, id)?;
    let mut changed = false;
    for turn_id in analysis_turn_ids {
        let turn_id = turn_id.trim();
        if turn_id.is_empty()
            || turn_id.chars().count() > MAX_TURN_ID_CHARS
            || !turn_id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err(EngineError::msg("invalid analysis turn id"));
        }
        if !record
            .analysis_turn_ids
            .iter()
            .any(|existing| existing == turn_id)
        {
            record.analysis_turn_ids.push(turn_id.to_owned());
            changed = true;
        }
    }
    if changed {
        record.updated_at_ms = now_ms().max(record.updated_at_ms.saturating_add(1));
        write_record(data_dir, &record)?;
    }
    Ok(record)
}

pub fn load(data_dir: &Path, id: &str) -> EngineResult<Investigation> {
    let path = record_path(data_dir, id)?;
    read_record(&path, id, data_dir)
}

pub fn list(data_dir: &Path) -> EngineResult<Vec<Investigation>> {
    let directory = investigations_dir(data_dir);
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(EngineError::io(
                format!("read {}", directory.display()),
                error,
            ));
        }
    };

    let mut records = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            EngineError::io(format!("read entries in {}", directory.display()), error)
        })?;
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|error| EngineError::io(format!("inspect {}", path.display()), error))?
            .is_file()
            || path.extension().and_then(|extension| extension.to_str()) != Some("json")
        {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| EngineError::msg("investigation filename is not valid UTF-8"))?;
        record_path(data_dir, id)?;
        records.push(read_record(&path, id, data_dir)?);
    }
    records.sort_by(|left, right| {
        right
            .updated_at_ms
            .cmp(&left.updated_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    fn scratch() -> PathBuf {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "fella-investigation-store-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn creates_loads_and_lists_independent_records() {
        let data_dir = scratch();
        let created = create(
            &data_dir,
            "/work/analysis",
            "  Monthly revenue  ",
            None,
            None,
            None,
        )
        .unwrap();

        assert!(created.id.starts_with("inv_"));
        assert_eq!(created.workspace_id, "/work/analysis");
        assert_eq!(created.title, "Monthly revenue");
        assert_eq!(created.brief, InvestigationBrief::default());
        assert_eq!(created.questions, Some(Vec::new()));
        assert!(created.analysis_turn_ids.is_empty());
        assert_eq!(created.status, InvestigationStatus::Active);
        assert_eq!(created.created_at_ms, created.updated_at_ms);
        assert_eq!(load(&data_dir, &created.id).unwrap(), created);
        assert_eq!(list(&data_dir).unwrap(), vec![created.clone()]);

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn source_selection_is_persisted_and_previous_snapshots_stay_unchanged() {
        let data_dir = scratch();
        let created = create(
            &data_dir,
            "/work/analysis",
            "Revenue investigation",
            Some("Explain revenue changes"),
            Some("message-root"),
            Some("conversation-root"),
        )
        .unwrap();
        let original_snapshot = context_snapshot(&data_dir, &created.id).unwrap();
        assert_eq!(
            original_snapshot.source_selection,
            InvestigationSourceSelection::default()
        );

        let updated = update_source_selection(
            &data_dir,
            &created.id,
            InvestigationSourceSelection {
                mode: InvestigationSourceSelectionMode::SelectedSources,
                included_paths: vec![r"exports\sales.csv".into()],
            },
        )
        .unwrap();
        let expected = InvestigationSourceSelection {
            mode: InvestigationSourceSelectionMode::SelectedSources,
            included_paths: vec!["exports/sales.csv".into()],
        };
        assert_eq!(updated.source_selection, expected);
        assert_eq!(load(&data_dir, &created.id).unwrap(), updated);
        assert_eq!(
            context_snapshot(&data_dir, &created.id)
                .unwrap()
                .source_selection,
            expected
        );
        assert_eq!(
            original_snapshot.source_selection,
            InvestigationSourceSelection::default(),
            "an already-captured run context is an immutable value"
        );

        assert!(update_source_selection(
            &data_dir,
            &created.id,
            InvestigationSourceSelection {
                mode: InvestigationSourceSelectionMode::SelectedSources,
                included_paths: vec!["../outside.csv".into()],
            },
        )
        .is_err());
        assert!(update_source_selection(
            &data_dir,
            &created.id,
            InvestigationSourceSelection {
                mode: InvestigationSourceSelectionMode::SelectedSources,
                included_paths: vec!["sales.csv".into(), "sales.csv".into()],
            },
        )
        .is_err());

        let all_sources = update_source_selection(
            &data_dir,
            &created.id,
            InvestigationSourceSelection {
                mode: InvestigationSourceSelectionMode::AllSources,
                included_paths: vec!["ignored.csv".into()],
            },
        )
        .unwrap();
        assert_eq!(
            all_sources.source_selection,
            InvestigationSourceSelection::default()
        );
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn question_plan_is_ordered_editable_and_does_not_rewrite_prior_run_links() {
        let data_dir = scratch();
        let created = create(
            &data_dir,
            "/work/analysis",
            "Conversion analysis",
            Some("Understand conversion by channel"),
            Some("message-root"),
            Some("conversation-root"),
        )
        .unwrap();
        let linked =
            link_analysis_turns(&data_dir, &created.id, &["turn-prior".to_owned()]).unwrap();
        let edited = update_questions(
            &data_dir,
            &created.id,
            vec![
                InvestigationQuestionInput {
                    id: "question-channel".to_owned(),
                    parent_id: None,
                    text: "Compare conversion rates by channel".to_owned(),
                },
                InvestigationQuestionInput {
                    id: "question-period".to_owned(),
                    parent_id: Some("question-channel".to_owned()),
                    text: "Check whether the pattern changed over time".to_owned(),
                },
            ],
        )
        .unwrap();

        assert_eq!(edited.questions.as_ref().unwrap()[0].id, "question-channel");
        assert_eq!(edited.questions.as_ref().unwrap()[1].id, "question-period");
        assert_eq!(edited.questions.as_ref().unwrap()[0].parent_id, None);
        assert_eq!(
            edited.questions.as_ref().unwrap()[1].parent_id.as_deref(),
            Some("question-channel")
        );
        assert_eq!(edited.analysis_turn_ids, linked.analysis_turn_ids);
        assert_eq!(load(&data_dir, &created.id).unwrap(), edited);
        assert_eq!(
            question_load(&data_dir, "question-period").unwrap(),
            edited.questions.as_ref().unwrap()[1]
        );

        let deliberately_empty = update_questions(&data_dir, &created.id, Vec::new()).unwrap();
        assert_eq!(deliberately_empty.questions, Some(Vec::new()));
        assert_eq!(
            question_load(&data_dir, "question-period").unwrap().status,
            InvestigationQuestionStatus::Removed
        );
        assert_eq!(
            update_questions(&data_dir, &created.id, Vec::new()).unwrap(),
            deliberately_empty
        );
        assert!(update_questions(
            &data_dir,
            &created.id,
            vec![
                InvestigationQuestionInput {
                    id: "duplicate".to_owned(),
                    parent_id: None,
                    text: "First".to_owned(),
                },
                InvestigationQuestionInput {
                    id: "duplicate".to_owned(),
                    parent_id: None,
                    text: "Second".to_owned(),
                },
            ],
        )
        .is_err());
        assert!(update_questions(
            &data_dir,
            &created.id,
            (0..3)
                .map(|index| InvestigationQuestionInput {
                    id: format!("long-{index}"),
                    parent_id: None,
                    text: "x".repeat(9_000),
                })
                .collect(),
        )
        .is_err());
        assert!(update_questions(
            &data_dir,
            &created.id,
            vec![InvestigationQuestionInput {
                id: "blank".to_owned(),
                parent_id: None,
                text: "  ".to_owned(),
            }],
        )
        .is_err());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn definitions_are_independent_versioned_and_removed_from_future_context() {
        let data_dir = scratch();
        let created = create(
            &data_dir,
            "/work/analysis",
            "Revenue analysis",
            Some("Explain revenue movement"),
            Some("message-root"),
            Some("conversation-root"),
        )
        .unwrap();
        let other = create(
            &data_dir,
            "/work/analysis",
            "Another investigation",
            None,
            None,
            None,
        )
        .unwrap();

        let saved = update_definitions(
            &data_dir,
            &created.id,
            vec![
                InvestigationDefinitionInput {
                    id: "definition-active-customer".into(),
                    kind: InvestigationDefinitionKind::Definition,
                    text: "An active customer ordered in the last 90 days.".into(),
                },
                InvestigationDefinitionInput {
                    id: "assumption-missing-cancel".into(),
                    kind: InvestigationDefinitionKind::Assumption,
                    text: "A missing cancellation date means the account is still active.".into(),
                },
            ],
        )
        .unwrap();
        assert_eq!(saved.definitions.as_ref().unwrap().len(), 2);
        assert_eq!(load(&data_dir, &created.id).unwrap(), saved);
        let original = definition_load(&data_dir, "definition-active-customer").unwrap();
        assert_eq!(original.revision, 1);
        assert!(original.history.is_empty());

        let snapshot = context_snapshot(&data_dir, &created.id).unwrap();
        assert_eq!(snapshot.investigation_id, created.id);
        assert_eq!(snapshot.definitions.len(), 2);
        assert!(question_with_context("Calculate revenue", &snapshot)
            .contains("active customer ordered in the last 90 days"));

        let edited = update_definitions(
            &data_dir,
            &created.id,
            vec![
                InvestigationDefinitionInput {
                    id: "definition-active-customer".into(),
                    kind: InvestigationDefinitionKind::Definition,
                    text: "An active customer placed a paid order in the last 60 days.".into(),
                },
                InvestigationDefinitionInput {
                    id: "assumption-missing-cancel".into(),
                    kind: InvestigationDefinitionKind::Assumption,
                    text: "A missing cancellation date means the account is still active.".into(),
                },
            ],
        )
        .unwrap();
        assert_eq!(edited.definitions.as_ref().unwrap().len(), 2);
        let revised = definition_load(&data_dir, "definition-active-customer").unwrap();
        assert_eq!(revised.revision, 2);
        assert_eq!(revised.history.len(), 1);
        assert_eq!(revised.history[0].revision, 1);
        assert_eq!(
            revised.history[0].text,
            "An active customer ordered in the last 90 days."
        );
        assert_eq!(
            context_snapshot(&data_dir, &created.id)
                .unwrap()
                .definitions[0]
                .revision,
            2
        );

        let removed = update_definitions(&data_dir, &created.id, Vec::new()).unwrap();
        assert_eq!(removed.definitions, Some(Vec::new()));
        assert_eq!(
            definition_load(&data_dir, "definition-active-customer")
                .unwrap()
                .status,
            InvestigationDefinitionStatus::Removed
        );
        assert_eq!(
            context_snapshot(&data_dir, &created.id)
                .unwrap()
                .definitions,
            Vec::new()
        );
        assert!(update_definitions(
            &data_dir,
            &other.id,
            vec![InvestigationDefinitionInput {
                id: "definition-active-customer".into(),
                kind: InvestigationDefinitionKind::Definition,
                text: "Must not cross Investigation ownership.".into(),
            }],
        )
        .is_err());
        assert!(update_definitions(
            &data_dir,
            &created.id,
            vec![
                InvestigationDefinitionInput {
                    id: "duplicate".into(),
                    kind: InvestigationDefinitionKind::Definition,
                    text: "First meaning".into(),
                },
                InvestigationDefinitionInput {
                    id: "duplicate".into(),
                    kind: InvestigationDefinitionKind::Assumption,
                    text: "Second meaning".into(),
                },
            ],
        )
        .is_err());
        assert!(update_definitions(
            &data_dir,
            &created.id,
            vec![InvestigationDefinitionInput {
                id: "blank".into(),
                kind: InvestigationDefinitionKind::Definition,
                text: "  ".into(),
            }],
        )
        .is_err());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn ensure_upgrades_a_legacy_investigation_with_its_existing_goal_as_a_question() {
        let data_dir = scratch();
        let legacy = create(
            &data_dir,
            "/work/analysis",
            "Legacy investigation",
            Some("Original analysis goal"),
            Some("message-root"),
            None,
        )
        .unwrap();
        let legacy_path = record_path(&data_dir, &legacy.id).unwrap();
        let old_record = serde_json::json!({
            "id": legacy.id,
            "workspace_id": legacy.workspace_id,
            "conversation_id": null,
            "questions": [
                { "id": "legacy-root", "text": "Original analysis goal" },
                { "id": "legacy-region", "text": "Compare the result by region" }
            ],
            "title": legacy.title,
            "brief": legacy.brief,
            "brief_source_message_id": legacy.brief_source_message_id,
            "brief_reviewed_message_id": legacy.brief_reviewed_message_id,
            "analysis_turn_ids": [],
            "status": "active",
            "created_at_ms": legacy.created_at_ms,
            "updated_at_ms": legacy.updated_at_ms
        });
        std::fs::write(
            &legacy_path,
            serde_json::to_vec_pretty(&old_record).unwrap(),
        )
        .unwrap();

        let upgraded = ensure_for_conversation(
            &data_dir,
            "/work/analysis",
            "conversation-root",
            "A later follow-up request",
            "message-root",
        )
        .unwrap();
        assert_eq!(upgraded.brief.goal, "Original analysis goal");
        assert_eq!(
            upgraded.conversation_id.as_deref(),
            Some("conversation-root")
        );
        assert_eq!(upgraded.questions.as_ref().unwrap().len(), 2);
        assert_eq!(
            upgraded.questions.as_ref().unwrap()[0].text,
            "Original analysis goal"
        );
        assert_eq!(upgraded.questions.as_ref().unwrap()[0].parent_id, None);
        assert_eq!(
            upgraded.questions.as_ref().unwrap()[1].parent_id.as_deref(),
            Some("legacy-root")
        );
        assert_eq!(
            question_load(&data_dir, "legacy-region")
                .unwrap()
                .investigation_id,
            upgraded.id
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn migration_restores_the_main_question_when_a_legacy_plan_was_empty() {
        let data_dir = scratch();
        let created = create(
            &data_dir,
            "/work/analysis",
            "Legacy investigation",
            Some("Original analysis goal"),
            Some("message-root"),
            None,
        )
        .unwrap();
        let legacy_path = record_path(&data_dir, &created.id).unwrap();
        let old_record = serde_json::json!({
            "id": created.id,
            "workspace_id": created.workspace_id,
            "conversation_id": null,
            "questions": [],
            "title": created.title,
            "brief": created.brief,
            "brief_source_message_id": created.brief_source_message_id,
            "brief_reviewed_message_id": created.brief_reviewed_message_id,
            "analysis_turn_ids": [],
            "status": "active",
            "created_at_ms": created.created_at_ms,
            "updated_at_ms": created.updated_at_ms
        });
        std::fs::write(
            &legacy_path,
            serde_json::to_vec_pretty(&old_record).unwrap(),
        )
        .unwrap();

        let migrated = load(&data_dir, &created.id).unwrap();
        let questions = migrated.questions.unwrap();
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].text, "Original analysis goal");
        assert!(questions[0].parent_id.is_none());
        assert_eq!(
            question_load(&data_dir, &questions[0].id).unwrap(),
            questions[0]
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn validates_model_question_breakdowns_before_showing_them_as_drafts() {
        let proposal = parse_question_breakdown(
            "```json\n{\"subquestions\":[\"Compare by channel\",\"Check the trend over time\"]}\n```",
            "How are sales changing?",
        )
        .unwrap();
        assert_eq!(proposal.subquestions.len(), 2);
        assert!(parse_question_breakdown(
            "{\"subquestions\":[\"How are sales changing?\",\"Compare by channel\"]}",
            "How are sales changing?",
        )
        .is_err());
        assert!(parse_question_breakdown(
            "{\"subquestions\":[\"Compare by channel\",\"Compare by channel\"]}",
            "How are sales changing?",
        )
        .is_err());
    }

    #[test]
    fn ensure_reuses_the_investigation_seeded_by_the_first_message() {
        let data_dir = scratch();
        let first = ensure_for_conversation(
            &data_dir,
            "/work/sales",
            "conversation-first",
            "  Summarize   sales trends  ",
            "message-first",
        )
        .unwrap();
        assert_eq!(first.title, "Summarize sales trends");
        assert_eq!(first.brief.goal, "Summarize   sales trends");
        assert_eq!(first.conversation_id.as_deref(), Some("conversation-first"));
        assert_eq!(
            first.brief_source_message_id.as_deref(),
            Some("message-first")
        );

        let reopened = ensure_for_conversation(
            &data_dir,
            "/work/sales",
            "conversation-first",
            "A follow-up should not replace the original request",
            "message-first",
        )
        .unwrap();
        assert_eq!(reopened, first);

        let manually_created = create(
            &data_dir,
            "/work/sales",
            "Analyst-selected title",
            Some("Original manual brief"),
            Some("message-manual"),
            None,
        )
        .unwrap();
        let resolved_manual = ensure_for_conversation(
            &data_dir,
            "/work/sales",
            "conversation-manual",
            "Do not replace the existing Investigation brief",
            "message-manual",
        )
        .unwrap();
        assert_eq!(resolved_manual.id, manually_created.id);
        assert_eq!(
            resolved_manual.conversation_id.as_deref(),
            Some("conversation-manual")
        );

        let another_conversation = ensure_for_conversation(
            &data_dir,
            "/work/sales",
            "conversation-second",
            "Compare sales by channel",
            "message-second",
        )
        .unwrap();
        assert_ne!(another_conversation.id, first.id);

        let same_message_in_another_workspace = ensure_for_conversation(
            &data_dir,
            "/work/other",
            "conversation-first",
            "Analyze the other folder",
            "message-first",
        )
        .unwrap();
        assert_ne!(same_message_in_another_workspace.id, first.id);
        assert_eq!(list(&data_dir).unwrap().len(), 4);
        assert!(ensure_for_conversation(
            &data_dir,
            "/work/sales",
            "  ",
            "Question",
            "message-invalid"
        )
        .is_err());
        assert!(ensure_for_conversation(
            &data_dir,
            "/work/sales",
            "conversation-conflict",
            "Question",
            "message-manual"
        )
        .is_err());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn defaults_blank_title_and_rejects_missing_scope_or_unsafe_ids() {
        let data_dir = scratch();
        let created = create(&data_dir, "/work/analysis", "  ", None, None, None).unwrap();
        assert_eq!(created.title, "New investigation");
        assert!(create(&data_dir, "  ", "No scope", None, None, None).is_err());
        assert!(load(&data_dir, "../conversation").is_err());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn missing_store_lists_as_empty() {
        let data_dir = scratch();
        assert!(list(&data_dir).unwrap().is_empty());
    }

    #[test]
    fn initial_request_drafts_an_editable_brief_without_overwriting_its_source_message() {
        let data_dir = scratch();
        let original_request = "  Compare retention by signup cohort and prepare a chart.  ";
        let created = create(
            &data_dir,
            "/work/analysis",
            "Retention",
            Some(original_request),
            Some("message-1"),
            None,
        )
        .unwrap();

        assert_eq!(
            created.brief.goal,
            "Compare retention by signup cohort and prepare a chart."
        );
        assert_eq!(created.brief.desired_output, "");
        assert!(created.brief.constraints.is_empty());
        assert_eq!(
            created.brief_source_message_id.as_deref(),
            Some("message-1")
        );
        assert_eq!(
            created.brief_reviewed_message_id.as_deref(),
            Some("message-1")
        );

        let updated = update_brief(
            &data_dir,
            &created.id,
            InvestigationBrief {
                goal: "Compare 30-day retention by signup cohort".into(),
                audience: "Product manager".into(),
                desired_output: "One chart and a short written summary".into(),
                constraints: vec!["Exclude test accounts".into(), "  ".into()],
            },
            Some("message-2"),
        )
        .unwrap();
        assert_eq!(
            updated.brief.goal,
            "Compare 30-day retention by signup cohort"
        );
        assert_eq!(updated.brief.audience, "Product manager");
        assert_eq!(
            updated.brief.desired_output,
            "One chart and a short written summary"
        );
        assert_eq!(updated.brief.constraints, vec!["Exclude test accounts"]);
        assert_eq!(
            updated.brief_source_message_id.as_deref(),
            Some("message-1")
        );
        assert_eq!(
            updated.brief_reviewed_message_id.as_deref(),
            Some("message-2")
        );
        assert!(updated.updated_at_ms > created.updated_at_ms);
        assert_eq!(load(&data_dir, &created.id).unwrap(), updated);
        assert_eq!(
            original_request.trim(),
            "Compare retention by signup cohort and prepare a chart."
        );
        let too_long = InvestigationBrief {
            goal: "x".repeat(12_001),
            ..updated.brief.clone()
        };
        assert!(update_brief(&data_dir, &created.id, too_long, None).is_err());
        assert_eq!(load(&data_dir, &created.id).unwrap(), updated);

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn reviewing_a_proposal_advances_only_the_review_cursor_when_dismissed() {
        let data_dir = scratch();
        let created = create(
            &data_dir,
            "/work/analysis",
            "Retention",
            Some("Compare retention by cohort"),
            Some("message-1"),
            None,
        )
        .unwrap();

        let dismissed = update_brief(
            &data_dir,
            &created.id,
            created.brief.clone(),
            Some("message-2"),
        )
        .unwrap();

        assert_eq!(dismissed.brief, created.brief);
        assert_eq!(
            dismissed.brief_source_message_id.as_deref(),
            Some("message-1")
        );
        assert_eq!(
            dismissed.brief_reviewed_message_id.as_deref(),
            Some("message-2")
        );
        assert_eq!(load(&data_dir, &created.id).unwrap(), dismissed);

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn analysis_turn_links_are_references_and_idempotent() {
        let data_dir = scratch();
        let created = create(
            &data_dir,
            "/work/analysis",
            "Retention",
            Some("Compare retention by cohort"),
            Some("message-1"),
            None,
        )
        .unwrap();
        let turn_ids = vec!["turn-one".to_string(), "turn-two".to_string()];

        let linked = link_analysis_turns(&data_dir, &created.id, &turn_ids).unwrap();
        assert_eq!(linked.analysis_turn_ids, turn_ids);
        assert_eq!(linked.brief, created.brief);
        assert_eq!(
            linked.brief_source_message_id,
            created.brief_source_message_id
        );
        assert!(linked.updated_at_ms > created.updated_at_ms);

        let linked_again =
            link_analysis_turns(&data_dir, &created.id, &["turn-one".to_string()]).unwrap();
        assert_eq!(linked_again.analysis_turn_ids, turn_ids);
        assert_eq!(linked_again.updated_at_ms, linked.updated_at_ms);
        assert_eq!(load(&data_dir, &created.id).unwrap(), linked);
        assert!(link_analysis_turns(&data_dir, &created.id, &["../outside".to_string()]).is_err());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn old_records_without_a_brief_load_with_an_empty_brief() {
        let data_dir = scratch();
        let created = create(&data_dir, "/work/analysis", "Legacy", None, None, None).unwrap();
        let path = record_path(&data_dir, &created.id).unwrap();
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("brief");
        json.as_object_mut()
            .unwrap()
            .remove("brief_source_message_id");
        json.as_object_mut()
            .unwrap()
            .remove("brief_reviewed_message_id");
        json.as_object_mut().unwrap().remove("analysis_turn_ids");
        json.as_object_mut().unwrap().remove("conversation_id");
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();

        assert_eq!(
            load(&data_dir, &created.id).unwrap().brief,
            InvestigationBrief::default()
        );
        assert_eq!(
            load(&data_dir, &created.id)
                .unwrap()
                .brief_reviewed_message_id,
            None
        );
        assert!(load(&data_dir, &created.id)
            .unwrap()
            .analysis_turn_ids
            .is_empty());
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
