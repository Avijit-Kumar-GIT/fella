//! Durable product record for one bounded unit of analyst work.
//!
//! The Investigation deliberately does not embed conversation messages or
//! analysis-turn payloads. Those records remain owned by their existing
//! stores and are linked in later product-model slices.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::engine::error::{EngineError, EngineResult};

const INVESTIGATIONS_DIR: &str = "investigations";

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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn read_record(path: &Path, expected_id: &str) -> EngineResult<Investigation> {
    let body = std::fs::read_to_string(path)
        .map_err(|error| EngineError::io(format!("read {}", path.display()), error))?;
    let record: Investigation = serde_json::from_str(&body).map_err(|error| {
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
        let record = Investigation {
            id: id.clone(),
            workspace_id: workspace_id.to_owned(),
            conversation_id: conversation_id.map(str::to_owned),
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
        let body = serde_json::to_string_pretty(&record).map_err(EngineError::from)?;
        let temporary = path.with_extension("json.tmp");
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(EngineError::io(
                    format!("create {}", temporary.display()),
                    error,
                ));
            }
        };
        if let Err(error) = file.write_all(body.as_bytes()) {
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
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(&temporary);
                continue;
            }
            Err(error) => {
                let _ = std::fs::remove_file(&temporary);
                return Err(EngineError::io(format!("write {}", path.display()), error));
            }
        }
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
        if let Some(linked_id) = existing.conversation_id.as_deref() {
            if linked_id != conversation_id {
                return Err(EngineError::msg(
                    "the source message is already linked to another conversation",
                ));
            }
        } else {
            // Upgrade records created before Conversation IDs were explicit.
            existing.conversation_id = Some(conversation_id.to_owned());
            existing.updated_at_ms = now_ms();
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

fn write_record(data_dir: &Path, record: &Investigation) -> EngineResult<()> {
    let path = record_path(data_dir, &record.id)?;
    let body = serde_json::to_vec_pretty(record).map_err(EngineError::from)?;
    let mut random = [0u8; 8];
    getrandom::fill(&mut random)
        .map_err(|error| EngineError::msg(format!("create investigation update id: {error}")))?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let temporary = path.with_file_name(format!(".{}-{suffix}.tmp", record.id));
    let write_result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| EngineError::io(format!("create {}", temporary.display()), error))?;
        file.write_all(&body)
            .map_err(|error| EngineError::io(format!("write {}", temporary.display()), error))?;
        file.sync_all()
            .map_err(|error| EngineError::io(format!("sync {}", temporary.display()), error))?;
        #[cfg(windows)]
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|error| EngineError::io(format!("replace {}", path.display()), error))?;
        }
        std::fs::rename(&temporary, &path)
            .map_err(|error| EngineError::io(format!("replace {}", path.display()), error))
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    write_result
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
    read_record(&path, id)
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
        records.push(read_record(&path, id)?);
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
        assert!(created.analysis_turn_ids.is_empty());
        assert_eq!(created.status, InvestigationStatus::Active);
        assert_eq!(created.created_at_ms, created.updated_at_ms);
        assert_eq!(load(&data_dir, &created.id).unwrap(), created);
        assert_eq!(list(&data_dir).unwrap(), vec![created.clone()]);

        let _ = std::fs::remove_dir_all(data_dir);
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
