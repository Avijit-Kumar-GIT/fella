//! End-to-end check of the data layer: scan a folder, load tables, query
//! them, enforce the read-only guard.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn scans_queries_and_guards_a_workspace() {
    let ws = scratch("ws");
    let data = scratch("data");

    fs::write(
        ws.join("sales.csv"),
        "month,amount\n2024-01,100\n2024-02,150\n2024-03,200\n",
    )
    .unwrap();
    fs::write(
        ws.join("people.json"),
        r#"[{"name":"ada","age":36},{"name":"grace","age":45}]"#,
    )
    .unwrap();
    fs::write(ws.join("notes.txt"), "just some prose, not a table\n").unwrap();
    fs::create_dir_all(ws.join("sub")).unwrap();
    fs::write(ws.join("sub").join("sales.csv"), "x\n1\n2\n").unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    assert_eq!(catalog.workspace.as_deref(), Some(ws.to_str().unwrap()));
    assert_eq!(catalog.sources.len(), 4);

    let sales = catalog
        .sources
        .iter()
        .find(|s| s.name == "sales.csv" && !s.path.contains("sub"))
        .unwrap();
    assert_eq!(sales.view.as_deref(), Some("sales"));
    assert_eq!(sales.row_count, Some(3));
    let cols: Vec<_> = sales
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(cols, vec!["month", "amount"]);

    // The nested sales.csv gets a de-duplicated view name.
    assert!(catalog
        .sources
        .iter()
        .any(|s| s.view.as_deref() == Some("sales_2")));

    // Non-tabular file is catalogued but has no view.
    let notes = catalog.sources.iter().find(|s| s.name == "notes.txt").unwrap();
    assert!(notes.view.is_none());

    // Query the view.
    let out = engine
        .run_sql("SELECT sum(amount) AS total FROM sales")
        .unwrap();
    assert_eq!(out.columns, vec!["total"]);
    assert_eq!(out.rows[0][0], serde_json::json!(450));

    // JSON source is queryable too.
    let out = engine
        .run_sql("SELECT count(*) AS n FROM people WHERE age > 40")
        .unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(1));

    // Read-only guard rejects mutations.
    assert!(engine.run_sql("DROP VIEW sales").is_err());
    assert!(engine.run_sql("INSERT INTO sales VALUES ('x', 1)").is_err());

    // describe() returns per-column stats.
    let described = engine.describe_source("sales").unwrap();
    let amount = described
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .find(|c| c.name == "amount")
        .unwrap();
    assert!(amount.null_fraction.is_some());
    assert!(amount.min.is_some());

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn messy_ledger_csv_coerces_currency_and_sums_right() {
    let ws = scratch("messy-ws");
    let data = scratch("messy-data");

    // Amounts written the way a person types them: currency signs, thousands
    // separators, one placeholder. A naive load would leave this column TEXT
    // and `SUM` would return near-0.
    fs::write(
        ws.join("ledger.csv"),
        "Date,Amount Paid,Method\n\
         2024-01-01,\"$1,200.00\",ACH\n\
         2024-02-01,\"1,200\",ACH\n\
         2024-03-01,1200,check\n\
         2024-04-01,N/A,\n\
         2024-05-01,\"$1,250.00\",ACH\n",
    )
    .unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    let ledger = catalog.sources.iter().find(|s| s.name == "ledger.csv").unwrap();
    let amount = ledger
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .find(|c| c.name == "Amount Paid")
        .unwrap();
    assert_eq!(amount.type_, "REAL", "currency text should be coerced to a number");
    assert!(amount.note.is_some(), "the coercion should be surfaced");

    let out = engine
        .run_sql(r#"SELECT sum("Amount Paid") AS total FROM ledger"#)
        .unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(4850.0));

    // The note survives a describe_schema round-trip and is cached back.
    let described = engine.describe_source("ledger").unwrap();
    assert!(described
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .find(|c| c.name == "Amount Paid")
        .unwrap()
        .note
        .is_some());

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[cfg(feature = "xlsx")]
#[test]
fn ingests_excel_sheets() {
    use fella_lib::engine::catalog::SourceKind;

    let ws = scratch("xlsx-ws");
    let data = scratch("xlsx-data");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/budget.xlsx");
    fs::copy(&fixture, ws.join("budget.xlsx")).unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    // One source per sheet; no bare workbook entry.
    let sheets: Vec<_> = catalog
        .sources
        .iter()
        .filter(|s| s.kind == SourceKind::Xlsx)
        .collect();
    assert_eq!(sheets.len(), 2, "expected one source per sheet");
    assert!(sheets.iter().all(|s| s.view.is_some()));

    let budget = catalog
        .sources
        .iter()
        .find(|s| s.name.contains("Budget"))
        .unwrap();
    let bview = budget.view.as_deref().unwrap();
    let cols: Vec<_> = budget
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(cols, vec!["category", "planned"]);

    let out = engine
        .run_sql(&format!("SELECT sum(planned) AS t FROM {bview}"))
        .unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(470));

    let actuals = catalog
        .sources
        .iter()
        .find(|s| s.name.contains("Actuals"))
        .unwrap();
    let aview = actuals.view.as_deref().unwrap();
    let out = engine
        .run_sql(&format!("SELECT round(sum(spent), 1) AS t FROM {aview}"))
        .unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(490.5));

    // describe works on the loaded table.
    let described = engine.describe_source(&budget.name).unwrap();
    assert_eq!(described.columns.as_ref().unwrap().len(), 2);

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[cfg(feature = "xlsx")]
#[test]
fn messy_ledger_xlsx_skips_preamble_coerces_currency_drops_total() {
    let ws = scratch("messy-xlsx-ws");
    let data = scratch("messy-xlsx-data");
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/messy_ledger.xlsx");
    fs::copy(&fixture, ws.join("messy_ledger.xlsx")).unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    let ledger = catalog
        .sources
        .iter()
        .find(|s| s.name.contains("Ledger"))
        .expect("the single sheet became a source");
    let view = ledger.view.as_deref().unwrap();

    // Title + spacer rows above the header are not consumed as data or columns.
    let cols: Vec<_> = ledger
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(cols, vec!["Date", "Amount Paid ($)", "Method"]);

    // Amounts written as "$1,200.00" / "1,150" alongside a bare 1200 and one
    // "N/A" are coerced to a real numeric column, and the coercion is surfaced.
    let amount = ledger
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .find(|c| c.name == "Amount Paid ($)")
        .unwrap();
    assert_eq!(amount.type_, "REAL", "currency text should coerce to a number");
    assert!(amount.note.is_some(), "the coercion should be noted");

    // The trailing "Total" row is dropped: 5 data rows, and the SUM is the true
    // total (1200 + 1150 + 1200 + 1250), not doubled by the summary line.
    assert_eq!(ledger.row_count, Some(5));
    let out = engine
        .run_sql(&format!(r#"SELECT sum("Amount Paid ($)") AS total FROM {view}"#))
        .unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(4800.0));

    // Both structural fixes are recorded on the source note.
    let note = ledger.note.as_deref().unwrap_or("");
    assert!(note.contains("preamble"), "note mentions the skipped preamble: {note:?}");
    assert!(note.contains("total"), "note mentions the dropped total row: {note:?}");

    // The column note survives a describe_source round-trip.
    let described = engine.describe_source(&ledger.name).unwrap();
    assert!(described
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .find(|c| c.name == "Amount Paid ($)")
        .unwrap()
        .note
        .is_some());

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn sniffs_a_semicolon_delimiter_and_strips_a_bom() {
    let ws = scratch("delim-ws");
    let data = scratch("delim-data");

    // Semicolon-delimited (common in Europe / bank exports) with a UTF-8 BOM
    // before the first header, the way Excel "CSV UTF-8" writes it.
    fs::write(
        ws.join("bank.csv"),
        "\u{feff}Date;Amount;Payee\n2024-01-01;12.50;Aldi\n2024-01-02;3.00;Bus\n",
    )
    .unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    let bank = catalog.sources.iter().find(|s| s.name == "bank.csv").unwrap();
    let cols: Vec<_> = bank
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(cols, vec!["Date", "Amount", "Payee"], "not one jammed column");
    assert!(bank.note.as_deref().unwrap_or("").contains("';'"), "delimiter noted: {:?}", bank.note);

    let out = engine.run_sql(r#"SELECT sum("Amount") AS t FROM bank"#).unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(15.5));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn reports_files_it_could_not_use() {
    let ws = scratch("skip-ws");
    let data = scratch("skip-data");

    fs::write(ws.join("good.csv"), "a,b\n1,2\n3,4\n").unwrap();
    fs::write(ws.join("notes.docx"), b"PK\x03\x04 not really a docx").unwrap();
    fs::write(ws.join("broken.json"), "{ this is not json").unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    // The good file still loads.
    assert!(catalog.sources.iter().any(|s| s.name == "good.csv" && s.view.is_some()));
    // The broken JSON is not fabricated as a source.
    assert!(!catalog.sources.iter().any(|s| s.name == "broken.json"));

    let skipped: Vec<&str> = catalog.skipped.iter().map(|f| f.name.as_str()).collect();
    assert!(skipped.contains(&"notes.docx"), "{skipped:?}");
    assert!(skipped.contains(&"broken.json"), "{skipped:?}");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn a_headerless_numeric_csv_keeps_its_first_row() {
    let ws = scratch("nohdr-ws");
    let data = scratch("nohdr-data");

    // No header the first row is data. Row 0 must not be eaten as column names.
    fs::write(ws.join("nums.csv"), "1,2\n3,4\n5,6\n").unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    let t = catalog.sources.iter().find(|s| s.name == "nums.csv").unwrap();
    assert_eq!(t.row_count, Some(3), "all three rows kept");
    let cols: Vec<_> =
        t.columns.as_ref().unwrap().iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cols, vec!["col1", "col2"], "synthesised names");

    let out = engine.run_sql("SELECT sum(col1) AS s FROM nums").unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(9));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn a_trailing_total_row_is_left_out_of_the_csv() {
    let ws = scratch("total-ws");
    let data = scratch("total-data");

    fs::write(
        ws.join("spend.csv"),
        "Month,Amount\nJan,100\nFeb,150\nMar,200\nGrand Total,450\n",
    )
    .unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    let t = catalog.sources.iter().find(|s| s.name == "spend.csv").unwrap();
    assert_eq!(t.row_count, Some(3), "the Grand Total line is not a data row");

    // SUM is the real 450, not doubled to 900.
    let out = engine.run_sql(r#"SELECT sum("Amount") AS s FROM spend"#).unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(450));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn csv_preamble_above_the_header_is_skipped() {
    let ws = scratch("preamble-ws");
    let data = scratch("preamble-data");

    // A report dump: title line, blank spacer, then the real header + data.
    fs::write(
        ws.join("report.csv"),
        "Monthly spending report,,\n,,\nMonth,Category,Amount\nJan,Food,100\nFeb,Food,120\n",
    )
    .unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    let t = catalog.sources.iter().find(|s| s.name == "report.csv").unwrap();
    let cols: Vec<_> =
        t.columns.as_ref().unwrap().iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cols, vec!["Month", "Category", "Amount"]);
    assert_eq!(t.row_count, Some(2));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}
