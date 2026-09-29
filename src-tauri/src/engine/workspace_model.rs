//! Revision-bound semantic facts derived from a mounted workspace.
//!
//! `Catalog` describes what Fella found. `WorkspaceModel` is the compact
//! analytical projection built from that catalog: fields receive cautious
//! candidate roles, while observed values and ingestion notes remain attached
//! to the source. Candidate roles are hints for interpretation, never facts
//! that can override the data or an explicit user definition.

use serde::{Deserialize, Serialize};

use crate::engine::catalog::{source_scope, Catalog, ColumnInfo, SkippedFile, SourceKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldRole {
    Date,
    Measure,
    Dimension,
    Identifier,
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldProfile {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub role: FieldRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub null_fraction: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distinct: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceModel {
    pub name: String,
    pub path: String,
    pub kind: SourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<i64>,
    pub fields: Vec<FieldProfile>,
    pub size_bytes: u64,
    pub mtime: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synopsis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A naming-based join hypothesis. Relationships are deliberately candidates,
/// not permissions or accepted join edges; grounding still has to resolve and
/// probe every join before execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipCandidate {
    pub left_source: String,
    pub left_field: String,
    pub right_source: String,
    pub right_field: String,
    pub evidence: String,
}

/// The semantic workspace snapshot used by later interpretation and planning
/// stages. Its revision is the same freshness boundary used by evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceModel {
    pub workspace: String,
    pub revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_at_ms: Option<i64>,
    pub sources: Vec<SourceModel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<RelationshipCandidate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<SkippedFile>,
}

impl WorkspaceModel {
    pub fn from_catalog(catalog: &Catalog) -> Option<Self> {
        let sources: Vec<SourceModel> = catalog
            .sources
            .iter()
            .map(SourceModel::from_catalog)
            .collect();
        Some(Self {
            workspace: catalog.workspace.clone()?,
            revision: catalog.revision.clone()?,
            indexed_at_ms: catalog.indexed_at_ms,
            relationships: infer_relationships(&sources),
            sources,
            skipped: catalog.skipped.clone(),
        })
    }

    pub fn source(&self, name: &str) -> Option<&SourceModel> {
        self.sources
            .iter()
            .find(|source| source.name == name || source.view.as_deref() == Some(name))
    }

    /// Render the revision-bound semantic profile for the model prompt. This
    /// is metadata for interpretation and planning, not evidence: observed
    /// figures still require a read-only tool call before they may appear in
    /// an answer.
    pub fn prompt_block(&self) -> String {
        let mut block = format!(
            "Workspace semantic profile (inferred metadata for revision {}; not answer evidence):\n",
            self.revision
        );
        for source in &self.sources {
            let name = source.view.as_deref().unwrap_or(&source.name);
            block.push_str(&format!(
                "  {name} (file={}, scope={}):\n",
                source.name,
                source_scope(&source.name, &source.path, source.view.as_deref()).label()
            ));
            if let Some(note) = &source.note {
                block.push_str(&format!("    source note: {}\n", prompt_value(note, 160)));
            }
            for field in &source.fields {
                block.push_str(&format!(
                    "    \"{}\" role={} type={}",
                    field.name,
                    role_name(field.role),
                    field.type_
                ));
                if let Some(null_fraction) = field.null_fraction {
                    block.push_str(&format!(" null={null_fraction:.2}"));
                }
                if let Some(distinct) = field.distinct {
                    block.push_str(&format!(" distinct={distinct}"));
                }
                if let Some(min) = &field.min {
                    block.push_str(&format!(" min={}", prompt_value(min, 60)));
                }
                if let Some(max) = &field.max {
                    block.push_str(&format!(" max={}", prompt_value(max, 60)));
                }
                if let Some(values) = &field.common_values {
                    let values = values
                        .iter()
                        .take(4)
                        .map(|value| prompt_value(value, 50))
                        .collect::<Vec<_>>()
                        .join(" | ");
                    if !values.is_empty() {
                        block.push_str(&format!(" common={values}"));
                    }
                }
                if let Some(note) = &field.note {
                    block.push_str(&format!(" note={}", prompt_value(note, 120)));
                }
                block.push('\n');
            }
        }
        if !self.relationships.is_empty() {
            block.push_str(
                "Candidate relationships (inferred naming hints; verify keys before joining):\n",
            );
            for relationship in self.relationships.iter().take(24) {
                block.push_str(&format!(
                    "  {}.{} ↔ {}.{} ({})\n",
                    relationship.left_source,
                    relationship.left_field,
                    relationship.right_source,
                    relationship.right_field,
                    relationship.evidence
                ));
            }
            if self.relationships.len() > 24 {
                block.push_str(&format!(
                    "  +{} more candidate relationship(s) omitted\n",
                    self.relationships.len() - 24
                ));
            }
        }
        block
    }
}

fn role_name(role: FieldRole) -> &'static str {
    match role {
        FieldRole::Date => "date",
        FieldRole::Measure => "measure",
        FieldRole::Dimension => "dimension",
        FieldRole::Identifier => "identifier",
        FieldRole::Text => "text",
    }
}

fn prompt_value(value: &str, limit: usize) -> String {
    value
        .replace(['\n', '\r'], " ")
        .chars()
        .take(limit)
        .collect()
}

impl SourceModel {
    fn from_catalog(source: &crate::engine::catalog::SourceInfo) -> Self {
        Self {
            name: source.name.clone(),
            path: source.path.clone(),
            kind: source.kind,
            view: source.view.clone(),
            row_count: source.row_count,
            fields: source
                .columns
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(FieldProfile::from_catalog)
                .collect(),
            size_bytes: source.size_bytes,
            mtime: source.mtime,
            synopsis: source.synopsis.clone(),
            note: source.note.clone(),
        }
    }
}

impl FieldProfile {
    fn from_catalog(column: &ColumnInfo) -> Self {
        Self {
            name: column.name.clone(),
            type_: column.type_.clone(),
            role: infer_role(column),
            null_fraction: column.null_fraction,
            distinct: column.distinct,
            min: column.min.clone(),
            max: column.max.clone(),
            example: column.example.clone(),
            common_values: column.common_values.clone(),
            note: column.note.clone(),
        }
    }
}

fn infer_relationships(sources: &[SourceModel]) -> Vec<RelationshipCandidate> {
    let mut relationships = Vec::new();
    for (left_index, left) in sources.iter().enumerate() {
        for right in sources.iter().skip(left_index + 1) {
            let left_source = source_key(left);
            let right_source = source_key(right);
            for left_field in &left.fields {
                for right_field in &right.fields {
                    let Some(evidence) =
                        relationship_evidence(left, left_field, right, right_field)
                    else {
                        continue;
                    };
                    relationships.push(RelationshipCandidate {
                        left_source: left_source.to_string(),
                        left_field: left_field.name.clone(),
                        right_source: right_source.to_string(),
                        right_field: right_field.name.clone(),
                        evidence: evidence.to_string(),
                    });
                }
            }
        }
    }
    relationships
}

fn relationship_evidence<'a>(
    left_source: &SourceModel,
    left: &FieldProfile,
    right_source: &SourceModel,
    right: &FieldProfile,
) -> Option<&'a str> {
    if !compatible_key_types(left, right)
        || (left.role != FieldRole::Identifier && right.role != FieldRole::Identifier)
    {
        return None;
    }
    let left_name = normalize_name(&left.name);
    let right_name = normalize_name(&right.name);
    if left_name == right_name && left_name != "id" {
        return Some("matching identifier names");
    }
    if left_name == "id" && entity_prefix_matches(&right_name, left_source) {
        return Some("right key names the left entity");
    }
    if right_name == "id" && entity_prefix_matches(&left_name, right_source) {
        return Some("left key names the right entity");
    }
    None
}

fn source_key(source: &SourceModel) -> &str {
    source.view.as_deref().unwrap_or(&source.name)
}

fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

fn entity_prefix_matches(field_name: &str, source: &SourceModel) -> bool {
    let source_name = normalize_name(source_key(source));
    let stem = source_name.strip_suffix('s').unwrap_or(&source_name);
    field_name
        .strip_suffix("id")
        .or_else(|| field_name.strip_suffix("key"))
        .is_some_and(|prefix| prefix == stem || prefix == source_name)
}

fn compatible_key_types(left: &FieldProfile, right: &FieldProfile) -> bool {
    let left_type = left.type_.to_ascii_lowercase();
    let right_type = right.type_.to_ascii_lowercase();
    let left_numeric = is_numeric_type(&left_type);
    let right_numeric = is_numeric_type(&right_type);
    left_numeric == right_numeric
}

fn infer_role(column: &ColumnInfo) -> FieldRole {
    let name = column.name.to_ascii_lowercase();
    let ty = column.type_.to_ascii_lowercase();

    if is_temporal_name(&name) || is_temporal_type(&ty) {
        return FieldRole::Date;
    }
    if is_identifier_name(&name) {
        return FieldRole::Identifier;
    }
    if is_numeric_type(&ty) {
        return FieldRole::Measure;
    }
    if column
        .common_values
        .as_ref()
        .is_some_and(|values| !values.is_empty())
        || is_dimension_name(&name)
    {
        return FieldRole::Dimension;
    }
    FieldRole::Text
}

fn is_temporal_name(name: &str) -> bool {
    [
        "date",
        "time",
        "timestamp",
        "datetime",
        "month",
        "year",
        "day",
    ]
    .iter()
    .any(|part| name == *part || name.contains(part))
}

fn is_temporal_type(ty: &str) -> bool {
    ["date", "time", "timestamp"]
        .iter()
        .any(|part| ty.contains(part))
}

fn is_identifier_name(name: &str) -> bool {
    name == "id"
        || name.ends_with("_id")
        || name.ends_with("_key")
        || name == "uuid"
        || name.ends_with("_uuid")
}

fn is_numeric_type(ty: &str) -> bool {
    [
        "int", "real", "float", "double", "decimal", "numeric", "number",
    ]
    .iter()
    .any(|part| ty.contains(part))
}

fn is_dimension_name(name: &str) -> bool {
    [
        "category", "type", "status", "segment", "region", "country", "city", "group", "class",
        "label",
    ]
    .iter()
    .any(|part| name == *part || name.contains(part))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::catalog::{Catalog, ColumnInfo, SourceInfo};

    fn catalog() -> Catalog {
        Catalog {
            workspace: Some("/tmp/analytics".into()),
            revision: Some("r123".into()),
            indexed_at_ms: Some(42),
            sources: vec![SourceInfo {
                name: "sales.csv".into(),
                path: "/tmp/analytics/sales.csv".into(),
                kind: SourceKind::Csv,
                view: Some("sales".into()),
                row_count: Some(12),
                columns: Some(vec![
                    ColumnInfo {
                        name: "sale_date".into(),
                        type_: "TEXT".into(),
                        null_fraction: Some(0.0),
                        distinct: Some(12),
                        min: None,
                        max: None,
                        example: Some("2026-01-01".into()),
                        common_values: None,
                        note: None,
                    },
                    ColumnInfo {
                        name: "amount".into(),
                        type_: "REAL".into(),
                        null_fraction: Some(0.0),
                        distinct: Some(12),
                        min: Some("1".into()),
                        max: Some("20".into()),
                        example: Some("10".into()),
                        common_values: None,
                        note: None,
                    },
                    ColumnInfo {
                        name: "region".into(),
                        type_: "TEXT".into(),
                        null_fraction: Some(0.0),
                        distinct: Some(2),
                        min: None,
                        max: None,
                        example: Some("East".into()),
                        common_values: Some(vec!["East".into(), "West".into()]),
                        note: None,
                    },
                ]),
                size_bytes: 100,
                mtime: 1,
                synopsis: None,
                note: None,
            }],
            skipped: Vec::new(),
        }
    }

    #[test]
    fn model_is_bound_to_catalog_revision() {
        let model = WorkspaceModel::from_catalog(&catalog()).unwrap();
        assert_eq!(model.revision, "r123");
        assert_eq!(model.source("sales").unwrap().row_count, Some(12));
        assert_eq!(model.sources[0].fields[0].role, FieldRole::Date);
        assert_eq!(model.sources[0].fields[1].role, FieldRole::Measure);
        assert_eq!(model.sources[0].fields[2].role, FieldRole::Dimension);
    }

    #[test]
    fn missing_workspace_or_revision_cannot_produce_a_model() {
        let mut no_revision = catalog();
        no_revision.revision = None;
        assert!(WorkspaceModel::from_catalog(&no_revision).is_none());

        let mut no_workspace = catalog();
        no_workspace.workspace = None;
        assert!(WorkspaceModel::from_catalog(&no_workspace).is_none());
    }

    #[test]
    fn model_serializes_as_an_inspectable_snapshot() {
        let value =
            serde_json::to_value(WorkspaceModel::from_catalog(&catalog()).unwrap()).unwrap();
        assert_eq!(value["revision"], "r123");
        assert_eq!(value["sources"][0]["fields"][1]["role"], "measure");
        assert!(value["sources"][0]["fields"][1]["common_values"].is_null());
    }

    #[test]
    fn prompt_block_exposes_roles_without_turning_metadata_into_evidence() {
        let block = WorkspaceModel::from_catalog(&catalog())
            .unwrap()
            .prompt_block();
        assert!(block.contains("revision r123"));
        assert!(block.contains("\"amount\" role=measure type=REAL"));
        assert!(block.contains("\"sale_date\" role=date"));
        assert!(block.contains("not answer evidence"));
    }

    #[test]
    fn infers_cautious_identifier_relationship_candidates() {
        let field = |name: &str, type_: &str| FieldProfile {
            name: name.into(),
            type_: type_.into(),
            role: FieldRole::Identifier,
            null_fraction: None,
            distinct: None,
            min: None,
            max: None,
            example: None,
            common_values: None,
            note: None,
        };
        let sources = vec![
            SourceModel {
                name: "customers.csv".into(),
                path: "/tmp/customers.csv".into(),
                kind: SourceKind::Csv,
                view: Some("customers".into()),
                row_count: None,
                fields: vec![field("id", "INTEGER")],
                size_bytes: 0,
                mtime: 0,
                synopsis: None,
                note: None,
            },
            SourceModel {
                name: "orders.csv".into(),
                path: "/tmp/orders.csv".into(),
                kind: SourceKind::Csv,
                view: Some("orders".into()),
                row_count: None,
                fields: vec![field("customer_id", "INTEGER")],
                size_bytes: 0,
                mtime: 0,
                synopsis: None,
                note: None,
            },
        ];
        let relationships = infer_relationships(&sources);
        assert_eq!(relationships.len(), 1);
        assert_eq!(relationships[0].left_source, "customers");
        assert_eq!(relationships[0].right_field, "customer_id");
        assert!(relationships[0].evidence.contains("right key"));
    }

    #[test]
    fn prompt_block_discloses_relationships_as_hints() {
        let mut model = WorkspaceModel::from_catalog(&catalog()).unwrap();
        model.relationships.push(RelationshipCandidate {
            left_source: "customers".into(),
            left_field: "id".into(),
            right_source: "orders".into(),
            right_field: "customer_id".into(),
            evidence: "right key names the left entity".into(),
        });
        let block = model.prompt_block();
        assert!(block.contains("Candidate relationships"));
        assert!(block.contains("customers.id ↔ orders.customer_id"));
        assert!(block.contains("verify keys before joining"));
    }
}
