//! Workspace scanning: walk a folder, classify files, derive view names.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::engine::error::{EngineError, EngineResult};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceScope {
    Current,
    Historical,
    Unknown,
}

/// A naming hint for source selection. It is intentionally only a prompt and
/// verification hint: the model still decides whether a source belongs in the
/// analysis, and the query remains the evidence.
pub fn source_scope(name: &str, path: &str, view: Option<&str>) -> SourceScope {
    let labels = format!("{} {} {}", name, view.unwrap_or_default(), path).to_ascii_lowercase();
    let tokens: Vec<&str> = labels
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    let historical = [
        "archive",
        "archived",
        "old",
        "legacy",
        "historical",
        "history",
        "backup",
        "snapshot",
        "prior",
        "previous",
    ];
    let current = ["current", "latest", "active", "live", "present", "primary"];
    if tokens.iter().any(|token| historical.contains(token)) {
        SourceScope::Historical
    } else if tokens.iter().any(|token| current.contains(token)) {
        SourceScope::Current
    } else {
        SourceScope::Unknown
    }
}

impl SourceScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "current/primary",
            Self::Historical => "historical/archive",
            Self::Unknown => "unspecified",
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// A few common values for low-cardinality label columns. This helps a
    /// person spot spelling/capitalisation differences before asking a query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_values: Option<Vec<String>>,
    /// Frequencies for the common values above. These are descriptive hints,
    /// not substitutes for a query when an exact result is needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_value_counts: Option<Vec<ValueFrequency>>,
    /// Ingest-time caveat about this column, e.g. amounts that were stored as
    /// text and coerced to numbers, or a column that looks numeric but was
    /// left as text. Surfaced in the schema digest and `inspect_table`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueFrequency {
    pub value: String,
    pub count: i64,
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
            common_values: None,
            common_value_counts: None,
            note: None,
        }
    }
}

/// Match a model-proposed field to a catalogued column without making fuzzy
/// guesses across unrelated fields. Exact normalized names always match. A
/// human request may also contain descriptive words around the physical field
/// (`gross revenue`, `average measured sleep duration`, `sensor reading`); a
/// shared meaningful token is a candidate in that case, and multiple candidate
/// columns remain ambiguous and are rejected by the caller. This is a
/// candidate matcher, not permission to silently choose a column.
pub(crate) fn field_name_matches(column: &str, requested: &str) -> bool {
    let column_normalized = normalize_field_name(column);
    let requested_normalized = normalize_field_name(requested);
    if column_normalized == requested_normalized {
        return true;
    }

    let requested_tokens = meaningful_field_tokens(requested);
    let column_tokens = meaningful_field_tokens(column);
    if requested_tokens.is_empty() || column_tokens.is_empty() {
        return false;
    }
    let overlap = requested_tokens
        .iter()
        .filter(|token| column_tokens.iter().any(|candidate| candidate == *token))
        .count();
    overlap > 0
}

/// Match a model-proposed source to either its display filename or its safe
/// query view. Source labels are allowed the same descriptive wrapper as field
/// labels, while the caller still rejects multiple matching sources.
pub(crate) fn source_name_matches(name: &str, view: Option<&str>, requested: &str) -> bool {
    let requested_normalized = normalize_field_name(requested);
    let requested_tokens = meaningful_field_tokens(requested);
    [Some(name), view].into_iter().flatten().any(|candidate| {
        let candidate_normalized = normalize_field_name(candidate);
        let candidate_tokens = meaningful_field_tokens(candidate);
        candidate_normalized == requested_normalized
            || requested_tokens
                .iter()
                .any(|token| candidate_tokens.iter().any(|candidate| candidate == token))
    })
}

pub(crate) fn source_name_exact_matches(name: &str, view: Option<&str>, requested: &str) -> bool {
    let requested_normalized = normalize_field_name(requested);
    [Some(name), view]
        .into_iter()
        .flatten()
        .any(|candidate| normalize_field_name(candidate) == requested_normalized)
}

/// Whether a source contains at least one field matching any model-proposed
/// request. A subject is often a human concept ("reading list", "rent")
/// rather than a physical filename; fields are the safer source binding.
pub(crate) fn source_has_any_field(source: &SourceInfo, requested: &[String]) -> bool {
    requested.iter().any(|name| {
        source
            .columns
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|column| field_name_matches(&column.name, name))
    })
}

fn normalize_field_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

fn field_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn meaningful_field_tokens(value: &str) -> Vec<String> {
    field_tokens(value)
        .into_iter()
        .filter(|token| token.len() >= 3)
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Bounded preview of a text document's leading content. `None` for tables
    /// and PDFs (avoids a parse at open time). Used as an initial clue, not as
    /// a substitute for reading the source when more context is needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synopsis: Option<String>,
    /// Ingest-time caveat about the whole source, e.g. preamble rows skipped
    /// above the header, or a trailing totals row dropped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Catalog {
    pub workspace: Option<String>,
    /// Deterministic identity of the currently loaded workspace snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Wall-clock time of the scan that produced this catalog, in Unix ms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_at_ms: Option<i64>,
    pub sources: Vec<SourceInfo>,
    /// Files the scan noticed but couldn't use, with a plain reason. Shown to
    /// the user so an incomplete dataset isn't analysed silently.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<SkippedFile>,
}

/// Lightweight progress for a workspace mount. `visited_files` counts
/// non-hidden, non-ignored files encountered; `supported_files` counts files
/// Fella will attempt to read. During preparation, `prepared_files` advances
/// against `total_supported_files`. Progress is advisory; the catalog is still
/// published atomically only after the full mount succeeds.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceProgress {
    pub phase: &'static str,
    pub visited_files: usize,
    pub supported_files: usize,
    pub prepared_files: usize,
    pub total_supported_files: Option<usize>,
    pub skipped_files: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingest: Option<WorkspaceIngestProgress>,
}

/// Byte progress for the source currently being profiled or loaded.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceIngestProgress {
    /// Workspace-relative path; stable even when basenames are duplicated.
    pub path: String,
    pub stage: &'static str,
    pub bytes_read: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileListFilter {
    All,
    Tables,
    Documents,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListPage {
    /// Rendered only for the requested page; never contains the full catalog.
    pub lines: Vec<String>,
    pub matching: usize,
    pub tables: usize,
    pub documents: usize,
    pub skipped: usize,
    pub offset: usize,
}

enum ListedItem<'a> {
    Source(&'a SourceInfo),
    Skipped(&'a SkippedFile),
}

struct OrderedListedItem<'a> {
    path: String,
    tie_breaker: String,
    category: u8,
    item: ListedItem<'a>,
}

/// Build a bounded inventory page directly from the mounted snapshot. Entries
/// use workspace-relative paths so duplicate basenames stay distinguishable.
pub(crate) fn list_files_page(
    root: &Path,
    sources: &[SourceInfo],
    skipped: &[SkippedFile],
    filter: FileListFilter,
    query: Option<&str>,
    offset: usize,
    limit: usize,
) -> FileListPage {
    const PAGE_CHAR_CAP: usize = 12_000;
    let tables = sources
        .iter()
        .filter(|source| source.view.is_some())
        .count();
    let documents = sources
        .iter()
        .filter(|source| source.view.is_none())
        .count();
    let skipped_count = skipped.len();
    let query = query.map(str::trim).filter(|query| !query.is_empty());
    let query_lower = query.map(str::to_lowercase);
    let mut entries = Vec::new();

    for source in sources {
        let category = if source.view.is_some() { 0 } else { 1 };
        if (category == 0 && matches!(filter, FileListFilter::Documents | FileListFilter::Skipped))
            || (category == 1 && matches!(filter, FileListFilter::Tables | FileListFilter::Skipped))
        {
            continue;
        }
        let path = workspace_relative_path(root, Path::new(&source.path));
        let tie_breaker = source.view.as_deref().unwrap_or(&source.name).to_string();
        if query_lower.as_ref().is_some_and(|needle| {
            !path.to_lowercase().contains(needle)
                && !source.name.to_lowercase().contains(needle)
                && !source
                    .view
                    .as_deref()
                    .is_some_and(|view| view.to_lowercase().contains(needle))
        }) {
            continue;
        }
        entries.push(OrderedListedItem {
            path,
            tie_breaker,
            category,
            item: ListedItem::Source(source),
        });
    }

    if !matches!(filter, FileListFilter::Tables | FileListFilter::Documents) {
        for file in skipped {
            if query_lower.as_ref().is_some_and(|needle| {
                !file.name.to_lowercase().contains(needle)
                    && !file.reason.to_lowercase().contains(needle)
            }) {
                continue;
            }
            entries.push(OrderedListedItem {
                path: file.name.clone(),
                tie_breaker: file.reason.clone(),
                category: 2,
                item: ListedItem::Skipped(file),
            });
        }
    }

    entries.sort_by(|left, right| {
        (&left.path, left.category, &left.tie_breaker).cmp(&(
            &right.path,
            right.category,
            &right.tie_breaker,
        ))
    });
    let matching = entries.len();
    let mut lines = Vec::with_capacity(limit);
    let mut rendered_chars = 0usize;
    for entry in entries.into_iter().skip(offset) {
        if lines.len() >= limit {
            break;
        }
        let line = match entry.item {
            ListedItem::Source(source) => match source.view.as_deref() {
                Some(view) => format!(
                    "table {view} (file={}, {} rows)",
                    entry.path,
                    source
                        .row_count
                        .map(|count| count.to_string())
                        .unwrap_or_else(|| "?".into())
                ),
                None => format!(
                    "document {} ({}, {} KB)",
                    entry.path,
                    source_kind_name(source.kind),
                    source.size_bytes / 1024
                ),
            },
            ListedItem::Skipped(file) => {
                format!(
                    "skipped {} ({})",
                    entry.path,
                    prompt_safe_text(&file.reason, 160)
                )
            }
        };
        let line_chars = line.chars().count() + usize::from(!lines.is_empty());
        if !lines.is_empty() && rendered_chars.saturating_add(line_chars) > PAGE_CHAR_CAP {
            break;
        }
        rendered_chars = rendered_chars.saturating_add(line_chars);
        lines.push(line);
    }

    FileListPage {
        lines,
        matching,
        tables,
        documents,
        skipped: skipped_count,
        offset,
    }
}

fn source_kind_name(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Csv => "CSV",
        SourceKind::Tsv => "TSV",
        SourceKind::Parquet => "Parquet",
        SourceKind::Json => "JSON",
        SourceKind::Ndjson => "NDJSON",
        SourceKind::Xlsx => "Excel",
        SourceKind::Pdf => "PDF",
        SourceKind::Text => "text",
    }
}

fn prompt_safe_text(value: &str, limit: usize) -> String {
    value
        .replace(['\n', '\r'], " ")
        .chars()
        .take(limit)
        .collect()
}

/// Derive a stable identity for the catalog that was actually loaded. This is
/// a freshness marker, not a cryptographic integrity hash: it changes when the
/// workspace path, source metadata, schema, ingest notes, or skipped files do.
pub fn workspace_revision(root: &Path, sources: &[SourceInfo], skipped: &[SkippedFile]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut feed = |value: &str| {
        for byte in value.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };

    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    feed(&canonical.to_string_lossy());

    let mut source_rows: Vec<String> = sources
        .iter()
        .map(|source| {
            let columns = source
                .columns
                .as_ref()
                .map(|columns| {
                    columns
                        .iter()
                        .map(|column| {
                            format!(
                                "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                                column.name,
                                column.type_,
                                column.note.as_deref().unwrap_or(""),
                                column
                                    .common_values
                                    .as_ref()
                                    .map(|values| values.join("\u{1d}"))
                                    .unwrap_or_default()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\u{1e}")
                })
                .unwrap_or_default();
            format!(
                "{}\u{1f}{}\u{1f}{:?}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
                source.name,
                source.path,
                source.kind,
                source.view.as_deref().unwrap_or(""),
                source.size_bytes,
                source.mtime,
                source.note.as_deref().unwrap_or(""),
                columns
            )
        })
        .collect();
    source_rows.sort();
    for row in source_rows {
        feed(&row);
    }

    let mut skipped_rows: Vec<String> = skipped
        .iter()
        .map(|file| format!("{}\u{1f}{}", file.name, file.reason))
        .collect();
    skipped_rows.sort();
    for row in skipped_rows {
        feed(&row);
    }

    if let Ok(metadata) = std::fs::metadata(root.join("fella.md")) {
        feed(&format!(
            "fella.md\u{1f}{}\u{1f}{:?}",
            metadata.len(),
            metadata.modified()
        ));
    }
    format!("r{hash:016x}")
}

/// A file that was found but not loaded (unsupported type, unreadable, or a
/// parse failure).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        "doc"
            | "docx"
            | "rtf"
            | "odt"
            | "pages"
            | "numbers"
            | "ods"
            | "eml"
            | "msg"
            | "html"
            | "htm"
            | "xml"
    )
}

pub(crate) fn workspace_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Walk `root`, returning recognised files sorted by path, plus a list of files
/// that were noticed but not loaded. Hidden entries and anything matched by a
/// `.fellaignore` in the root are skipped silently. Traversal is recursive with
/// no implicit depth limit and never follows symlinks.
pub fn scan(root: &Path) -> EngineResult<(Vec<ScannedFile>, Vec<SkippedFile>)> {
    scan_with_progress(root, |_| {})
}

/// Inventory the tree and periodically report counts without opening file
/// contents. Callers that only need a complete vector can use `scan`.
pub(crate) fn scan_with_progress(
    root: &Path,
    mut on_progress: impl FnMut(WorkspaceProgress),
) -> EngineResult<(Vec<ScannedFile>, Vec<SkippedFile>)> {
    if !root.is_dir() {
        return Err(EngineError::msg(format!(
            "That doesn't look like a folder: {}",
            root.display()
        )));
    }

    let ignore = Ignore::load(root);
    let mut out = Vec::new();
    let mut skipped: Vec<SkippedFile> = Vec::new();
    let mut visited_files = 0usize;
    let mut skipped_files = 0usize;

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_hidden(e.file_name().to_str()))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                if let Some(p) = err.path() {
                    skipped.push(SkippedFile {
                        name: workspace_relative_path(root, p),
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
        // `fella.md` at the workspace root is user context (see the workspace
        // system), not a data file skip it the way `.fellaignore` is skipped.
        if path.parent() == Some(root) && path.file_name() == Some(std::ffi::OsStr::new("fella.md"))
        {
            continue;
        }
        visited_files += 1;
        let ext = path.extension().and_then(|e| e.to_str());
        let Some(kind) = ext.and_then(SourceKind::from_ext) else {
            if ext.is_some_and(worth_mentioning) {
                skipped.push(SkippedFile {
                    name: workspace_relative_path(root, path),
                    reason: "Fella can't read this file type yet".into(),
                });
                skipped_files += 1;
            }
            report_scan_progress(&mut on_progress, visited_files, out.len(), skipped_files);
            continue;
        };
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                skipped.push(SkippedFile {
                    name: workspace_relative_path(root, path),
                    reason: "couldn't be read (permission, or open in another app)".into(),
                });
                skipped_files += 1;
                report_scan_progress(&mut on_progress, visited_files, out.len(), skipped_files);
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
        report_scan_progress(&mut on_progress, visited_files, out.len(), skipped_files);
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    skipped.sort_by(|a, b| a.name.cmp(&b.name));
    skipped.dedup_by(|a, b| a.name == b.name);
    on_progress(WorkspaceProgress {
        phase: "scanning",
        visited_files,
        supported_files: out.len(),
        prepared_files: 0,
        total_supported_files: Some(out.len()),
        skipped_files: skipped.len(),
        ingest: None,
    });
    Ok((out, skipped))
}

fn report_scan_progress(
    on_progress: &mut impl FnMut(WorkspaceProgress),
    visited_files: usize,
    supported_files: usize,
    skipped_files: usize,
) {
    if visited_files.is_multiple_of(128) {
        on_progress(WorkspaceProgress {
            phase: "scanning",
            visited_files,
            supported_files,
            prepared_files: 0,
            total_supported_files: None,
            skipped_files,
            ingest: None,
        });
    }
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
        self.patterns
            .iter()
            .any(|p| p == name || rel_str == *p || rel_str.starts_with(&format!("{p}/")))
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
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempTree(PathBuf);

    impl TempTree {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "fella-catalog-{label}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

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

    #[test]
    fn scan_finds_supported_data_beyond_eight_nested_directories() {
        let tree = TempTree::new("deep");
        let mut deep = tree.path().to_path_buf();
        for level in 0..12 {
            deep.push(format!("level-{level}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("observations.csv"), "value\n7\n").unwrap();

        let (sources, _) = scan(tree.path()).unwrap();
        assert_eq!(sources.len(), 1);
        assert!(sources[0].path.ends_with("observations.csv"));
    }

    #[test]
    fn scan_keeps_same_named_skips_in_distinct_folders() {
        let tree = TempTree::new("duplicate-skips");
        for folder in ["alpha", "beta"] {
            let dir = tree.path().join(folder);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("report.docx"), "not parsed").unwrap();
        }

        let (_, skipped) = scan(tree.path()).unwrap();
        let names: HashSet<&str> = skipped.iter().map(|file| file.name.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains("alpha/report.docx"));
        assert!(names.contains("beta/report.docx"));
    }

    #[test]
    fn scan_inventories_five_thousand_files_and_reports_elapsed_time() {
        let tree = TempTree::new("scale");
        for directory in 0..100 {
            let dir = tree.path().join(format!("batch-{directory:03}"));
            std::fs::create_dir_all(&dir).unwrap();
            for file in 0..50 {
                std::fs::write(dir.join(format!("row-{file:02}.csv")), "v\n1\n").unwrap();
            }
        }

        let started = std::time::Instant::now();
        let (sources, skipped) = scan(tree.path()).unwrap();
        let elapsed = started.elapsed();
        eprintln!(
            "catalog inventory baseline: files={} skipped={} elapsed_ms={}",
            sources.len(),
            skipped.len(),
            elapsed.as_millis()
        );
        assert_eq!(sources.len(), 5_000);
        assert!(skipped.is_empty());
    }

    #[test]
    fn field_matching_handles_human_unit_labels_without_guessing_short_names() {
        assert!(field_name_matches("Sleep (hrs)", "sleep"));
        assert!(field_name_matches("Amount Paid", "amount"));
        assert!(field_name_matches("sleep_hours", "sleep_hours"));
        assert!(field_name_matches("Revenue", "gross revenue"));
        assert!(field_name_matches(
            "Sleep (hrs)",
            "average measured sleep duration"
        ));
        assert!(field_name_matches("reading", "sensor reading"));
        assert!(!field_name_matches("Customer ID", "id"));
        assert!(!field_name_matches("Sleep (hrs)", "duration"));
    }

    #[test]
    fn source_matching_accepts_a_view_or_descriptive_label() {
        assert!(source_name_matches(
            "forecast.json",
            Some("forecast"),
            "forecast"
        ));
        assert!(source_name_matches(
            "Bank Export - current.csv",
            Some("bank_export_current"),
            "current bank export"
        ));
        assert!(!source_name_matches("sales.csv", Some("sales"), "health"));
    }

    #[test]
    fn source_scope_distinguishes_current_archive_and_unknown_files() {
        assert_eq!(
            source_scope("current.csv", "/tmp/workspace/current.csv", Some("current")),
            SourceScope::Current
        );
        assert_eq!(
            source_scope("old.csv", "/tmp/workspace/archive/old.csv", Some("old")),
            SourceScope::Historical
        );
        assert_eq!(
            source_scope("sales.csv", "/tmp/workspace/sales.csv", Some("sales")),
            SourceScope::Unknown
        );
    }

    #[test]
    fn workspace_revision_changes_when_loaded_source_changes() {
        let source = SourceInfo {
            name: "sales.csv".into(),
            path: "/tmp/sales.csv".into(),
            kind: SourceKind::Csv,
            view: Some("sales".into()),
            row_count: Some(2),
            columns: Some(vec![ColumnInfo::bare("amount", "REAL")]),
            size_bytes: 20,
            mtime: 1,
            synopsis: None,
            note: None,
        };
        let first = workspace_revision(
            Path::new("/tmp/workspace"),
            std::slice::from_ref(&source),
            &[],
        );
        let mut changed = source;
        changed.size_bytes += 1;
        let second = workspace_revision(Path::new("/tmp/workspace"), &[changed], &[]);
        assert_ne!(first, second);
        assert!(first.starts_with('r'));
    }
}
