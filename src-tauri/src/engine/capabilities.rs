//! User-selectable analysis capabilities.
//!
//! This is intentionally a small policy surface. It controls which existing
//! analysis paths the agent may use; it is not a plugin registry or a general
//! permission system. Workspace discovery, evidence capture, verification,
//! and the read-only boundary remain part of the core harness.

use serde::{Deserialize, Serialize};

fn enabled_by_default() -> bool {
    true
}

/// The analysis paths currently available in the personal product.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisCapabilities {
    /// Schema inspection, table samples, and read-only SQL.
    #[serde(default = "enabled_by_default")]
    pub table_analysis: bool,
    /// Search and reading for catalogued text and PDF sources.
    #[serde(default = "enabled_by_default")]
    pub document_analysis: bool,
    /// Sandboxed RustPython/WASM calculations.
    #[serde(default = "enabled_by_default")]
    pub python_analysis: bool,
    /// Structured, validated chart generation.
    #[serde(default = "enabled_by_default")]
    pub visualizations: bool,
}

impl Default for AnalysisCapabilities {
    fn default() -> Self {
        Self {
            table_analysis: true,
            document_analysis: true,
            python_analysis: true,
            visualizations: true,
        }
    }
}

impl AnalysisCapabilities {
    /// Charts currently depend on table queries, so the effective chart
    /// capability is narrower than the stored visualization switch.
    pub fn charts_enabled(self) -> bool {
        self.table_analysis && self.visualizations
    }

    /// A short model-facing explanation when one or more paths are disabled.
    pub fn prompt_notice(self) -> Option<String> {
        let mut disabled = Vec::new();
        if !self.table_analysis {
            disabled.push("table analysis (schema inspection and SQL)");
        }
        if !self.document_analysis {
            disabled.push("document analysis (search and reading)");
        }
        if !self.python_analysis {
            disabled.push("Python analysis");
        }
        if !self.visualizations {
            disabled.push("visualizations");
        } else if !self.table_analysis {
            disabled.push("visualizations (they require table analysis)");
        }

        (!disabled.is_empty()).then(|| format!("Disabled analysis paths: {}.", disabled.join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_the_current_analysis_surface() {
        let caps = AnalysisCapabilities::default();
        assert!(caps.table_analysis);
        assert!(caps.document_analysis);
        assert!(caps.python_analysis);
        assert!(caps.visualizations);
        assert!(caps.charts_enabled());
        assert!(caps.prompt_notice().is_none());
    }

    #[test]
    fn chart_dependency_and_prompt_notice_are_explicit() {
        let caps = AnalysisCapabilities {
            table_analysis: false,
            ..AnalysisCapabilities::default()
        };
        assert!(!caps.charts_enabled());
        let notice = caps.prompt_notice().unwrap();
        assert!(notice.contains("table analysis"));
        assert!(notice.contains("visualizations (they require table analysis)"));
    }
}
