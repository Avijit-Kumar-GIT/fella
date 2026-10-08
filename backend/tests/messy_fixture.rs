//! Regression coverage for the human-shaped bank export used in manual v1 QA.

use std::fs;
use std::path::{Path, PathBuf};

use fella_lib::engine::grounding;
use fella_lib::engine::planner;
use fella_lib::engine::runtime::{AnalysisContract, ContractMeasure, InterpretationStatus};
use fella_lib::engine::state::EngineState;

fn scratch(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "fella-{label}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn remove(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

#[test]
fn messy_bank_export_keeps_safe_annotations_and_drops_summary_row() {
    let ws = scratch("messy-bank-export-ws");
    let data = scratch("messy-bank-export-data");

    fs::write(
        ws.join("Bank Export - current.csv"),
        r#"Date;Description;Amount;Category;Account;Cleared
2025-01-03;Corner Market;"$82.14";Groceries;Checking;yes
2025-01-05;ACME Electric;"$124.60";Bills;Checking;yes
2025-01-08;Metro card reload;"$40.00";Transport;Checking;yes
2025-01-12;Salary;"$3,200.00";Income;Checking;yes
2025-01-15;Corner Market;"$61.08";groceries;Checking;yes
2025-01-21;Refund - Shoes;"-$45.00";Shopping;Checking;yes
2025-01-25;Landlord;"$1,500.00";Housing;Checking;yes
2025-01-28;Coffee + bagel;"$9.50";Food;Checking;no
2025-02-03;Corner Market;"$94.22";Groceries;Checking;yes
2025-02-06;ACME Electric;N/A;Bills;Checking;pending
2025-02-09;Freelance invoice;"$850.00";Income;Checking;yes
2025-02-14;Landlord;"$1,500.00";Housing;Checking;yes
2025-02-19;Bike repair;"$210.00 (cash)";Transport;Checking;yes
2025-02-22;Dinner with Maya;"$68.40";Food;Checking;no
Total;;"$7,694.94";;;
"#,
    )
    .unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();
    let source = catalog
        .sources
        .iter()
        .find(|source| source.name == "Bank Export - current.csv")
        .unwrap();
    assert_eq!(source.row_count, Some(14));
    let amount = source
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .find(|column| column.name == "Amount")
        .unwrap();
    assert_eq!(amount.type_, "REAL");

    let view = source.view.as_deref().unwrap();
    let out = engine
        .run_sql(&format!(
            r#"SELECT SUM("Amount") AS net_spending FROM {view} WHERE lower("Category") <> 'income'"#
        ))
        .unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(3644.94));

    let missing = engine
        .run_sql(&format!(
            r#"SELECT COUNT(*) - COUNT("Amount") AS missing FROM {view}"#
        ))
        .unwrap();
    assert_eq!(missing.rows[0][0], serde_json::json!(1));

    remove(&ws);
    remove(&data);
}

#[test]
fn humanized_sleep_label_can_be_grounded_and_averaged() {
    let ws = scratch("messy-health-ws");
    let data = scratch("messy-health-data");

    fs::write(
        ws.join("Health log (raw).csv"),
        r#"Date,Workout,Minutes,Sleep (hrs),Mood
2025-01-03,run,30,7.5,good
01/04/2025,rest,,6.0,meh
"Jan 5, 2025",weights,45,N/A,good
2025-01-06,yoga,20,7.2,good
2025-01-07,long walk,60,8.0,good
2025-01-08,run,35,6.8,rough
2025-01-09,rest,0,,meh
"#,
    )
    .unwrap();

    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();
    let grounded = grounding::ground(
        &engine,
        AnalysisContract {
            interpretation: InterpretationStatus::Assumed,
            measures: vec![ContractMeasure {
                concept: "sleep".into(),
                field: Some("sleep".into()),
                operation: "average".into(),
                unit: Some("hours".into()),
            }],
            ..Default::default()
        },
    );
    assert_eq!(
        grounded.contract.interpretation,
        InterpretationStatus::Grounded
    );
    assert_eq!(
        grounded.contract.measures[0].field.as_deref(),
        Some("Sleep (hrs)")
    );

    let plan = planner::compile(
        &engine.catalog(),
        &grounded.contract,
        grounded.report.source.as_deref(),
    )
    .unwrap();
    let result = engine.run_sql(&plan.sql).unwrap();
    assert_eq!(result.rows[0][0], serde_json::json!(7.1));

    remove(&ws);
    remove(&data);
}

#[test]
fn json_sources_and_human_labels_join_the_same_contract_path() {
    let ws = scratch("messy-mixed-sources-ws");
    let data = scratch("messy-mixed-sources-data");

    fs::write(
        ws.join("forecast.json"),
        r#"[
  {"month":"2025-01","plan":4800,"actual":5000},
  {"month":"2025-02","plan":5100,"actual":4900}
]"#,
    )
    .unwrap();
    fs::write(
        ws.join("health log.csv"),
        "Date,Sleep (hrs),Quality\n2025-01-01,7.5,good\n2025-01-02,N/A,poor\n2025-01-03,6.5,good\n",
    )
    .unwrap();

    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();

    let forecast = grounding::ground(
        &engine,
        AnalysisContract {
            interpretation: InterpretationStatus::Assumed,
            subject: Some("forecast source".into()),
            measures: vec![
                ContractMeasure {
                    concept: "actual revenue".into(),
                    field: Some("actual".into()),
                    operation: "sum".into(),
                    unit: None,
                },
                ContractMeasure {
                    concept: "planned revenue".into(),
                    field: Some("plan".into()),
                    operation: "sum".into(),
                    unit: None,
                },
            ],
            derived_metrics: vec![fella_lib::engine::runtime::ContractDerivedMetric {
                concept: "actual minus plan".into(),
                kind: fella_lib::engine::runtime::DerivedMetricKind::Difference,
                numerator: "actual revenue".into(),
                denominator: "planned revenue".into(),
                unit: None,
            }],
            ..Default::default()
        },
    );
    assert_eq!(
        forecast.contract.interpretation,
        InterpretationStatus::Grounded
    );
    assert_eq!(forecast.report.source.as_deref(), Some("forecast"));
    let plan = planner::compile(
        &engine.catalog(),
        &forecast.contract,
        forecast.report.source.as_deref(),
    )
    .unwrap();
    let result = engine.run_sql(&plan.sql).unwrap();
    assert_eq!(result.rows[0][0], serde_json::json!(9900));
    assert_eq!(result.rows[0][2], serde_json::json!(0));

    let sleep = grounding::ground(
        &engine,
        AnalysisContract {
            interpretation: InterpretationStatus::Assumed,
            measures: vec![ContractMeasure {
                concept: "average measured sleep duration".into(),
                field: Some("average measured sleep duration".into()),
                operation: "average".into(),
                unit: Some("hours".into()),
            }],
            ..Default::default()
        },
    );
    assert_eq!(
        sleep.contract.interpretation,
        InterpretationStatus::Grounded
    );
    assert_eq!(
        sleep.contract.measures[0].field.as_deref(),
        Some("Sleep (hrs)")
    );

    remove(&ws);
    remove(&data);
}
