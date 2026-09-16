//! Map SQL-backed evidence to the catalogued files and tables it used.
//!
//! This is deliberately a small, deterministic mapping rather than a SQL
//! parser. View names are generated safe identifiers, and the verifier already
//! uses the same conservative `FROM`/`JOIN` relation scan for its checks.

use crate::engine::catalog::Catalog;
use crate::engine::evidence::EvidenceSource;

/// Return catalogued sources referenced by a read-only query.
pub fn for_sql(catalog: &Catalog, sql: &str) -> Vec<EvidenceSource> {
    let relations = super::verify::referenced_relations(sql);
    catalog
        .sources
        .iter()
        .filter_map(|source| {
            let table = source.view.as_ref()?;
            relations
                .contains(&table.to_lowercase())
                .then(|| EvidenceSource {
                    table: table.clone(),
                    source: source.name.clone(),
                    note: source.note.clone(),
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::catalog::{ColumnInfo, SourceInfo, SourceKind};

    fn catalog() -> Catalog {
        Catalog {
            workspace: Some("/tmp/ws".into()),
            revision: Some("rtest".into()),
            sources: vec![
                SourceInfo {
                    name: "sales.csv".into(),
                    path: "/tmp/sales.csv".into(),
                    kind: SourceKind::Csv,
                    view: Some("sales".into()),
                    row_count: Some(2),
                    columns: Some(vec![ColumnInfo::bare("amount", "REAL")]),
                    size_bytes: 1,
                    mtime: 1,
                    synopsis: None,
                    note: Some("amounts were coerced".into()),
                },
                SourceInfo {
                    name: "notes.md".into(),
                    path: "/tmp/notes.md".into(),
                    kind: SourceKind::Text,
                    view: None,
                    row_count: None,
                    columns: None,
                    size_bytes: 1,
                    mtime: 1,
                    synopsis: None,
                    note: None,
                },
                SourceInfo {
                    name: "expenses.csv".into(),
                    path: "/tmp/expenses.csv".into(),
                    kind: SourceKind::Csv,
                    view: Some("expenses".into()),
                    row_count: Some(2),
                    columns: Some(vec![ColumnInfo::bare("amount", "REAL")]),
                    size_bytes: 1,
                    mtime: 1,
                    synopsis: None,
                    note: None,
                },
            ],
            skipped: Vec::new(),
        }
    }

    #[test]
    fn maps_from_and_join_tables_to_catalogued_sources() {
        let refs = for_sql(
            &catalog(),
            "SELECT * FROM SALES s JOIN expenses e ON s.id = e.id",
        );
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].table, "sales");
        assert_eq!(refs[0].source, "sales.csv");
        assert_eq!(refs[0].note.as_deref(), Some("amounts were coerced"));
        assert_eq!(refs[1].table, "expenses");
        assert_eq!(refs[1].source, "expenses.csv");
    }
}
