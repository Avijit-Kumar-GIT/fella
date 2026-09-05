//! Workspace scanning: walk a folder, classify files, derive view names.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::engine::error::{EngineError, EngineResult};

/// How deep below the workspace root we look for files. `FELLA_SCAN_DEPTH`
/// overrides. People nest a year or two of subfolders; 3 was too shallow.
fn max_depth() -> usize {
    super::env::positive("FELLA_SCAN_DEPTH", 8)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Csv,
    Tsv,
    Parquet,
    Json,
    Ndjson,
    Xlsx,
    Pdf,
    Text,
}

impl SourceKind {
    pub fn from_ext(ext: &str) -> Option<Self> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "csv" => Self::Csv,
            "tsv" | "tab" => Self::Tsv,
            "parquet" | "pq" => Self::Parquet,
            "json" => Self::Json,
            "ndjson" | "jsonl" => Self::Ndjson,
            "xlsx" | "xlsm" | "xlsb" | "xls" => Self::Xlsx,
            "pdf" => Self::Pdf,
            "txt" | "text" | "md" | "markdown" | "log" => Self::Text,
            _ => return None,
        })
    }

    /// True for kinds that become a queryable DuckDB view.
    pub fn is_tabular(self) -> bool {
        matches!(
            self,
            Self::Csv | Self::Tsv | Self::Parquet | Self::Json | Self::Ndjson | Self::Xlsx
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
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
    /// Ingest-time caveat about this column, e.g. amounts that were stored as
    /// text and coerced to numbers, or a column that looks numeric but was
    /// left as text. Surfaced in the schema digest and `describe_schema`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ColumnInfo {
    /// A bare column: name + type, no stats, no note.
    pub fn bare(name: impl Into<String>, type_: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_: type_.into(),
            null_fraction: None,
            distinct: None,
            min: None,
            max: None,
            example: None,
            note: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    pub name: String,
    pub path: String,
    pub kind: SourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<ColumnInfo>>,
    pub size_bytes: u64,
    pub mtime: i64,
    /// One-line preview of a text document's first non-empty line. `None` for
    /// tables and for PDFs (avoids a parse at open time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synopsis: Option<String>,
    /// Ingest-time caveat about the whole source, e.g. preamble rows skipped
    /// above the header, or a trailing totals row dropped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Catalog {
    pub workspace: Option<String>,
    pub sources: Vec<SourceInfo>,
    /// Files the scan noticed but couldn't use, with a plain reason. Shown to
    /// the user so an incomplete dataset isn't analysed silently.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<SkippedFile>,
}

/// A file that was found but not loaded (unsupported type, unreadable, or a
/// parse failure).
#[derive(Debug, Clone, Serialize)]
pub struct SkippedFile {
    pub name: String,
    pub reason: String,
}

/// A file found during a scan, before any DuckDB work.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub kind: SourceKind,
    pub size_bytes: u64,
    pub mtime: i64,
}

/// Extensions a person would reasonably expect Fella to read but that it
/// doesn't (yet). Worth telling them about; images / archives / media / code
/// are not.
fn worth_mentioning(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "doc" | "docx" | "rtf" | "odt" | "pages" | "numbers" | "ods" | "eml" | "msg"
            | "html" | "htm" | "xml"
    )
}

fn file_name(p: &Path) -> String {
    p.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string()
}

/// Walk `root`, returning recognised files sorted by path, plus a list of files
/// that were noticed but not loaded. Hidden entries and anything matched by a
/// `.fellaignore` in the root are skipped silently.
pub fn scan(root: &Path) -> EngineResult<(Vec<ScannedFile>, Vec<SkippedFile>)> {
    if !root.is_dir() {
        return Err(EngineError::msg(format!(
            "That doesn't look like a folder: {}",
            root.display()
        )));
    }

    let ignore = Ignore::load(root);
    let mut out = Vec::new();
    let mut skipped: Vec<SkippedFile> = Vec::new();

    for entry in WalkDir::new(root)
        .max_depth(max_depth())
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_hidden(e.file_name().to_str()))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                if let Some(p) = err.path() {
                    skipped.push(SkippedFile {
                        name: file_name(p),
                        reason: "couldn't be read (permission, or open in another app)".into(),
                    });
                }
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if ignore.matches(root, path) {
            continue;
        }
        // `fella.md` at the workspace root is user context (see the extensions
        // system), not a data file skip it the way `.fellaignore` is skipped.
        if path.parent() == Some(root)
            && path.file_name() == Some(std::ffi::OsStr::new("fella.md"))
        {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str());
        let Some(kind) = ext.and_then(SourceKind::from_ext) else {
            if ext.is_some_and(worth_mentioning) {
                skipped.push(SkippedFile {
                    name: file_name(path),
                    reason: "Fella can't read this file type yet".into(),
                });
            }
            continue;
        };
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                skipped.push(SkippedFile {
                    name: file_name(path),
                    reason: "couldn't be read (permission, or open in another app)".into(),
                });
                continue;
            }
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        out.push(ScannedFile {
            path: path.to_path_buf(),
            kind,
            size_bytes: meta.len(),
            mtime,
        });
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    skipped.sort_by(|a, b| a.name.cmp(&b.name));
    skipped.dedup_by(|a, b| a.name == b.name);
    Ok((out, skipped))
}

fn is_hidden(name: Option<&str>) -> bool {
    matches!(name, Some(n) if n.starts_with('.') && n != "." && n != "..")
}

/// Minimal ignore file: one pattern per line, `#` comments. A pattern matches
/// if it equals the file name or is a path prefix of the workspace-relative
/// path. No globs deliberately tiny.
struct Ignore {
    patterns: Vec<String>,
}

impl Ignore {
    fn load(root: &Path) -> Self {
        let text = std::fs::read_to_string(root.join(".fellaignore")).unwrap_or_default();
        let patterns = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.trim_end_matches('/').to_string())
            .collect();
        Self { patterns }
    }

    fn matches(&self, root: &Path, path: &Path) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let rel = path.strip_prefix(root).unwrap_or(path);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        self.patterns.iter().any(|p| {
            p == name
                || rel_str == *p
                || rel_str.starts_with(&format!("{p}/"))
        })
    }
}

/// Turn a file stem into a safe DuckDB identifier: lowercase, non-alphanumerics
/// to `_`, collapsed and trimmed, never starting with a digit.
pub fn slugify(stem: &str) -> String {
    let mut s: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    let s = s.trim_matches('_').to_string();
    if s.is_empty() {
        "source".to_string()
    } else if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("t_{s}")
    } else {
        s
    }
}

/// Pick a unique view name for `stem`, recording it in `used`.
pub fn unique_view_name(stem: &str, used: &mut HashSet<String>) -> String {
    let base = slugify(stem);
    if used.insert(base.clone()) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basics() {
        assert_eq!(slugify("Sales Report 2024"), "sales_report_2024");
        assert_eq!(slugify("weird--name..v2"), "weird_name_v2");
        assert_eq!(slugify("123abc"), "t_123abc");
        assert_eq!(slugify("  "), "source");
        assert_eq!(slugify("__x__"), "x");
    }

    #[test]
    fn dedupes_view_names() {
        let mut used = HashSet::new();
        assert_eq!(unique_view_name("sales", &mut used), "sales");
        assert_eq!(unique_view_name("sales", &mut used), "sales_2");
        assert_eq!(unique_view_name("Sales!", &mut used), "sales_3");
    }

    #[test]
    fn ext_classification() {
        assert_eq!(SourceKind::from_ext("CSV"), Some(SourceKind::Csv));
        assert_eq!(SourceKind::from_ext("jsonl"), Some(SourceKind::Ndjson));
        assert_eq!(SourceKind::from_ext("xlsx"), Some(SourceKind::Xlsx));
        assert_eq!(SourceKind::from_ext("db"), None);
        assert!(SourceKind::Parquet.is_tabular());
        assert!(!SourceKind::Pdf.is_tabular());
    }
}
