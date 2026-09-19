//! Exhaustive checks for the experimental analysis capability policy.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::capabilities::AnalysisCapabilities;
use fella_lib::engine::tools::Registry;
use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&path).unwrap();
    path
}

fn capabilities(mask: u8) -> AnalysisCapabilities {
    AnalysisCapabilities {
        table_analysis: mask & 0b0001 != 0,
        document_analysis: mask & 0b0010 != 0,
        python_analysis: mask & 0b0100 != 0,
        visualizations: mask & 0b1000 != 0,
    }
}

fn names(registry: &Registry) -> Vec<String> {
    registry
        .schemas()
        .into_iter()
        .map(|schema| schema.name)
        .collect()
}

#[tokio::test]
async fn every_capability_combination_matches_registry_and_engine_guards() {
    let workspace = scratch("capability-matrix-workspace");
    let data = scratch("capability-matrix-data");
    fs::write(
        workspace.join("sales.csv"),
        "category,amount\nRent,1250\nGroceries,412.5\n",
    )
    .unwrap();
    fs::write(workspace.join("notes.txt"), "The lease renews in June.\n").unwrap();

    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&workspace).unwrap();
    let chart_args = serde_json::json!({
        "kind": "bar",
        "sql": "SELECT category, amount FROM sales ORDER BY amount DESC"
    });

    for mask in 0..16 {
        let caps = capabilities(mask);
        let patch = serde_json::json!({
            "capabilities": {
                "table_analysis": caps.table_analysis,
                "document_analysis": caps.document_analysis,
                "python_analysis": caps.python_analysis,
                "visualizations": caps.visualizations
            }
        });
        engine.save_settings(patch.as_object().unwrap()).unwrap();
        assert_eq!(engine.settings().capabilities, caps, "mask {mask:04b}");

        let registry = Registry::standard_with(caps);
        let tool_names = names(&registry);
        assert!(tool_names.iter().any(|name| name == "list_files"));
        assert_eq!(
            tool_names.iter().any(|name| name == "inspect_table"),
            caps.table_analysis
        );
        assert_eq!(
            tool_names.iter().any(|name| name == "run_sql"),
            caps.table_analysis
        );
        assert_eq!(
            tool_names.iter().any(|name| name == "grep_files"),
            caps.document_analysis
        );
        assert_eq!(
            tool_names.iter().any(|name| name == "read_file"),
            caps.document_analysis
        );
        assert_eq!(
            tool_names.iter().any(|name| name == "run_python"),
            caps.python_analysis
        );
        assert_eq!(
            tool_names.iter().any(|name| name == "make_chart"),
            caps.table_analysis && caps.visualizations,
            "mask {mask:04b}"
        );

        assert_eq!(
            engine.run_sql("SELECT count(*) FROM sales").is_ok(),
            caps.table_analysis,
            "SQL mask {mask:04b}"
        );
        assert_eq!(
            engine.describe_source("sales").is_ok(),
            caps.table_analysis,
            "describe mask {mask:04b}"
        );
        assert_eq!(
            engine.grep_files("lease", 10).is_ok(),
            caps.document_analysis,
            "grep mask {mask:04b}"
        );
        assert_eq!(
            engine.read_file("notes.txt").is_ok(),
            caps.document_analysis,
            "read mask {mask:04b}"
        );
        match (
            caps.table_analysis && caps.visualizations,
            registry.run(&engine, "make_chart", &chart_args).await,
        ) {
            (true, Some(Ok(output))) => assert!(output.chart.is_some(), "mask {mask:04b}"),
            (false, None) => {}
            (true, Some(Err(error))) => {
                panic!("chart should run for mask {mask:04b}: {error}")
            }
            (true, None) => panic!("chart tool missing for mask {mask:04b}"),
            (false, Some(Ok(_))) => panic!("chart ran for mask {mask:04b}"),
            (false, Some(Err(error))) => {
                panic!("chart should be unavailable for mask {mask:04b}: {error}")
            }
        }
    }

    // Python guest startup is intentionally covered once for each policy
    // state in the focused Python integration suite. The matrix above still
    // checks Python exposure for all 16 masks without making every matrix
    // row pay the guest startup cost.
    engine
        .save_settings(
            serde_json::json!({
                "capabilities": {
                    "table_analysis": true,
                    "document_analysis": true,
                    "python_analysis": false,
                    "visualizations": true
                }
            })
            .as_object()
            .unwrap(),
        )
        .unwrap();
    assert!(engine.run_python("print(42)").await.is_err());
    engine
        .save_settings(
            serde_json::json!({
                "capabilities": {
                    "table_analysis": true,
                    "document_analysis": true,
                    "python_analysis": true,
                    "visualizations": true
                }
            })
            .as_object()
            .unwrap(),
        )
        .unwrap();
    assert!(engine.run_python("print(42)").await.is_ok());

    let _ = fs::remove_dir_all(&workspace);
    let _ = fs::remove_dir_all(&data);
}
