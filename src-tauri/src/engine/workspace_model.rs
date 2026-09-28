//! Revision-bound semantic facts derived from a mounted workspace.
//!
//! `Catalog` describes what Fella found. `WorkspaceModel` is the compact
//! analytical projection built from that catalog: fields receive cautious
//! candidate roles, while observed values and ingestion notes remain attached
//! to the source. Candidate roles are hints for interpretation, never facts
//! that can override the data or an explicit user definition.

use serde::{Deserialize, Serialize};

use crate::engine::catalog::{Catalog, ColumnInfo, SkippedFile, SourceKind};

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
    pub skipped: Vec<SkippedFile>,
}

impl WorkspaceModel {
    pub fn from_catalog(catalog: &Catalog) -> Option<Self> {
        Some(Self {
            workspace: catalog.workspace.clone()?,
            revision: catalog.revision.clone()?,
            indexed_at_ms: catalog.indexed_at_ms,
            sources: catalog
                .sources
                .iter()
                .map(SourceModel::from_catalog)
                .collect(),
            skipped: catalog.skipped.clone(),
        })
    }

    pub fn source(&self, name: &str) -> Option<&SourceModel> {
        self.sources
            .iter()
            .find(|source| source.name == name || source.view.as_deref() == Some(name))
    }
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
}
