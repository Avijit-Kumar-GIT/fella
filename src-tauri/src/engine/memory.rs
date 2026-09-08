//! Per-folder learned notes: a plain-Markdown file Fella keeps for a workspace,
//! written from what it *verifies* and what the user *corrects*, and that the
//! user can read and edit. Not a memory system it's the learned sibling of
//! `fella.md`.
//!
//! One `.md` per workspace under `<data_dir>/memory/`, plus an append-only
//! `.episodes.jsonl` (the raw record; not yet read back). The `.md` is the
//! source of truth: parsing is lenient (an unrecognised line inside a managed
//! section is kept verbatim), and a user-authored `## Notes` section and any
//! sections we don't know are preserved on rewrite.
//!
//! `FELLA_MEMORY=0` turns the whole thing off (read and write).

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Hard cap on stored recipes; the least-used is dropped past this.
const MAX_RECIPES: usize = 40;
/// Episodes kept in the `.jsonl` before the oldest are trimmed.
const MAX_EPISODES: usize = 200;
/// Character budget for the block prepended to the system prompt.
const CORE_BUDGET: usize = 1600;
/// How many recipes (most-used first) go in that block.
const CORE_RECIPES: usize = 5;

/// On unless `FELLA_MEMORY=0`.
pub fn enabled() -> bool {
    !matches!(std::env::var("FELLA_MEMORY").as_deref(), Ok("0"))
}

/// `<data_dir>/memory/<basename>-<hash>.md` for a workspace path. Stable across
/// runs (hand-rolled FNV-1a `DefaultHasher` isn't guaranteed stable on disk).
pub fn path_for(data_dir: &Path, workspace: &Path) -> PathBuf {
    let canon = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in canon.as_os_str().to_string_lossy().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let base = canon
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.chars().filter(|c| c.is_alphanumeric()).take(24).collect::<String>())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "folder".into());
    data_dir.join("memory").join(format!("{base}-{h:016x}.md"))
}

#[derive(Clone, PartialEq, Eq)]
struct Note {
    key: String,
    text: String,
}

#[derive(Clone)]
struct Recipe {
    question: String,
    sql: String,
    tables: Vec<String>,
    uses: u32,
    date: String,
    stale: bool,
}

#[derive(Default)]
pub struct FolderMemory {
    path: PathBuf,
    preferences: Vec<String>,
    vocabulary: Vec<Note>,
    tables: Vec<Note>,
    recipes: Vec<Recipe>,
    /// The user's `## Notes` section, verbatim.
    freeform: Vec<String>,
    /// Sections we don't manage, verbatim, re-emitted after ours.
    trailer: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    None,
    Pref,
    Vocab,
    Tables,
    Recipes,
    Freeform,
    Trailer,
}

impl FolderMemory {
    /// Load from disk; an absent file is an empty memory.
    pub fn load(path: &Path) -> Self {
        let mut m = FolderMemory { path: path.to_path_buf(), ..Default::default() };
        let Ok(text) = std::fs::read_to_string(path) else { return m };
        let mut sec = Section::None;
        let mut lines = text.lines().peekable();
        while let Some(raw) = lines.next() {
            if sec == Section::Trailer {
                m.trailer.push(raw.to_string());
                continue;
            }
            let line = raw.trim_end();
            if let Some(h) = line.strip_prefix("## ") {
                sec = match h.trim().to_lowercase().as_str() {
                    "preferences" => Section::Pref,
                    "vocabulary" => Section::Vocab,
                    "table notes" | "tables" => Section::Tables,
                    "recipes" => Section::Recipes,
                    "notes" => Section::Freeform,
                    _ => {
                        m.trailer.push(raw.to_string());
                        Section::Trailer
                    }
                };
                continue;
            }
            match sec {
                Section::None => {} // H1, comments, blank lead-in
                Section::Pref => {
                    let t = line.strip_prefix("- ").unwrap_or(line).trim();
                    if !t.is_empty() {
                        m.preferences.push(t.to_string());
                    }
                }
                Section::Vocab | Section::Tables => {
                    let Some(item) = line.strip_prefix("- ") else { continue };
                    let (key, txt) = split_note(item);
                    let note = Note { key, text: txt };
                    if sec == Section::Vocab {
                        m.vocabulary.push(note);
                    } else {
                        m.tables.push(note);
                    }
                }
                Section::Recipes => {
                    let Some(head) = line.strip_prefix("- ") else { continue };
                    let mut r = parse_recipe_head(head);
                    let mut sql = String::new();
                    while let Some(peek) = lines.peek() {
                        if peek.trim().is_empty() || peek.starts_with("- ") || peek.starts_with("## ")
                        {
                            break;
                        }
                        if !sql.is_empty() {
                            sql.push('\n');
                        }
                        sql.push_str(peek.trim_start());
                        lines.next();
                    }
                    r.sql = sql;
                    if !r.question.is_empty() && !r.sql.is_empty() {
                        m.recipes.push(r);
                    }
                }
                Section::Freeform => m.freeform.push(raw.to_string()),
                Section::Trailer => unreachable!(),
            }
        }
        m
    }

    /// Rewrite the file in canonical form (managed sections first, then the
    /// user's `## Notes` and any unknown sections verbatim). Best-effort a
    /// write failure is logged, not fatal.
    pub fn save(&self) {
        if self.is_empty() {
            let _ = std::fs::remove_file(&self.path);
            return;
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut s = String::new();
        s.push_str("# Fella  learned notes for this folder\n\n");
        s.push_str(
            "<!-- Fella writes this from queries it verified and corrections you gave.\n     \
Edit or delete anything; delete the file to start over.\n     \
The \"## Notes\" section and any sections you add are left untouched. -->\n",
        );
        if !self.preferences.is_empty() {
            s.push_str("\n## Preferences\n");
            for p in &self.preferences {
                s.push_str(&format!("- {p}\n"));
            }
        }
        if !self.vocabulary.is_empty() {
            s.push_str("\n## Vocabulary\n");
            for n in &self.vocabulary {
                s.push_str(&render_note(n));
            }
        }
        if !self.tables.is_empty() {
            s.push_str("\n## Table notes\n");
            for n in &self.tables {
                s.push_str(&render_note(n));
            }
        }
        if !self.recipes.is_empty() {
            s.push_str("\n## Recipes\n");
            for r in &self.recipes {
                let flag = if r.stale { "  (stale: a table is gone)" } else { "" };
                s.push_str(&format!(
                    "- {}  \u{b7}  used {}\u{d7}  \u{b7}  {}  \u{b7}  {}{flag}\n",
                    r.question,
                    r.uses,
                    r.tables.join(", "),
                    r.date,
                ));
                for l in r.sql.lines() {
                    s.push_str(&format!("    {l}\n"));
                }
            }
        }
        if !self.freeform.is_empty() {
            s.push_str("\n## Notes\n");
            for l in &self.freeform {
                s.push_str(l);
                s.push('\n');
            }
        }
        for l in &self.trailer {
            s.push_str(l);
            s.push('\n');
        }
        if let Ok(mut f) = std::fs::File::create(&self.path) {
            if let Err(e) = f.write_all(s.as_bytes()) {
                log::warn!("memory: write {}: {e}", self.path.display());
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.preferences.is_empty()
            && self.vocabulary.is_empty()
            && self.tables.is_empty()
            && self.recipes.is_empty()
            && self.freeform.iter().all(|l| l.trim().is_empty())
            && self.trailer.is_empty()
    }

    /// Mark recipes whose tables are no longer in the workspace.
    pub fn mark_stale(&mut self, known_views: &[String]) {
        for r in &mut self.recipes {
            r.stale = !r.tables.iter().all(|t| known_views.iter().any(|k| k == t));
        }
    }

    /// Replace-or-insert a `key`ed note (vocabulary or table); newest text wins.
    fn upsert(list: &mut Vec<Note>, key: &str, text: &str) {
        let key = key.trim();
        let text = text.trim();
        if key.is_empty() || text.is_empty() {
            return;
        }
        match list.iter_mut().find(|n| n.key.eq_ignore_ascii_case(key)) {
            Some(n) => n.text = text.to_string(),
            None => list.push(Note { key: key.to_string(), text: text.to_string() }),
        }
    }

    pub fn set_table_note(&mut self, key: &str, text: &str) {
        Self::upsert(&mut self.tables, key, text);
    }

    pub fn set_vocab(&mut self, key: &str, text: &str) {
        Self::upsert(&mut self.vocabulary, key, text);
    }

    /// Record a query that verified for a question. Same (normalised) question
    /// bumps the use-count and refreshes the SQL; a new one is appended and the
    /// least-used trimmed past `MAX_RECIPES`.
    pub fn record_recipe(&mut self, question: &str, sql: &str, tables: &[String]) {
        let q = normalise_question(question);
        let sql = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        if q.is_empty() || sql.is_empty() {
            return;
        }
        if let Some(r) = self.recipes.iter_mut().find(|r| normalise_question(&r.question) == q) {
            r.uses += 1;
            r.sql = sql;
            r.date = today_ymd();
            r.tables = tables.to_vec();
            r.stale = false;
            return;
        }
        self.recipes.push(Recipe {
            question: question.trim().to_string(),
            sql,
            tables: tables.to_vec(),
            uses: 1,
            date: today_ymd(),
            stale: false,
        });
        if self.recipes.len() > MAX_RECIPES {
            // drop the least-used, oldest-on-tie
            if let Some((i, _)) = self
                .recipes
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.uses.cmp(&b.1.uses).then(a.1.date.cmp(&b.1.date)))
            {
                self.recipes.remove(i);
            }
        }
    }

    /// The block prepended to the system prompt: preferences, all vocabulary,
    /// all (non-stale) table notes, and the most-used recipes, within
    /// `CORE_BUDGET` chars (recipes drop first). `None` if there's nothing.
    pub fn semantic_core(&self) -> Option<String> {
        let mut p = String::new();
        if !self.preferences.is_empty() {
            p.push_str("Preferences:\n");
            for x in &self.preferences {
                p.push_str(&format!("- {x}\n"));
            }
        }
        if !self.vocabulary.is_empty() {
            p.push_str("What the user's words mean here:\n");
            for n in &self.vocabulary {
                p.push_str(&format!("- {} \u{2192} {}\n", n.key, n.text));
            }
        }
        let live_notes: Vec<&Note> = self.tables.iter().collect();
        if !live_notes.is_empty() {
            p.push_str("Table notes:\n");
            for n in live_notes {
                if n.key.is_empty() {
                    p.push_str(&format!("- {}\n", n.text));
                } else {
                    p.push_str(&format!("- {}: {}\n", n.key, n.text));
                }
            }
        }
        let mut recipes: Vec<&Recipe> = self.recipes.iter().filter(|r| !r.stale).collect();
        recipes.sort_by(|a, b| b.uses.cmp(&a.uses).then(b.date.cmp(&a.date)));
        for (i, r) in recipes.into_iter().take(CORE_RECIPES).enumerate() {
            let entry = format!("- \"{}\"\n    {}\n", r.question, r.sql.replace('\n', " "));
            if i > 0 && p.len() + entry.len() > CORE_BUDGET {
                break;
            }
            if i == 0 {
                p.push_str("SQL that verified before for this folder (adapt it, don't paste blindly):\n");
            }
            p.push_str(&entry);
        }
        if p.is_empty() {
            return None;
        }
        Some(format!(
            "Learned notes for this folder (reference, not rules the user can edit them in memory.md):\n{p}"
        ))
    }
}

/// Append one line to `<mem>.episodes.jsonl`, trimming to the last
/// `MAX_EPISODES`. The raw record; nothing reads it back yet.
pub fn record_episode(mem_path: &Path, line: &serde_json::Value) {
    let p = episodes_path(mem_path);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut lines: Vec<String> = std::fs::read_to_string(&p)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    lines.push(line.to_string());
    let n = lines.len();
    if n > MAX_EPISODES {
        lines.drain(0..n - MAX_EPISODES);
    }
    let _ = std::fs::write(&p, lines.join("\n") + "\n");
}

fn episodes_path(mem_path: &Path) -> PathBuf {
    mem_path.with_extension("episodes.jsonl")
}

/// A follow-up that plainly corrects the previous answer (not a new question).
pub fn is_correction(q: &str) -> bool {
    let l = q.trim().to_lowercase();
    const MARKERS: [&str; 10] = [
        "no,", "no ", "actually", "that's wrong", "that's not right", "that is wrong",
        "wrong,", "should be", "it's actually", "correction:",
    ];
    MARKERS.iter().any(|m| l.starts_with(m))
}

// --- helpers -------------------------------------------------------------

fn split_note(item: &str) -> (String, String) {
    for sep in [" \u{2192} ", " \u{2014} ", " - ", ": "] {
        if let Some((k, v)) = item.split_once(sep) {
            return (k.trim().to_string(), v.trim().to_string());
        }
    }
    (String::new(), item.trim().to_string())
}

fn render_note(n: &Note) -> String {
    if n.key.is_empty() {
        format!("- {}\n", n.text)
    } else {
        format!("- {} \u{2014} {}\n", n.key, n.text)
    }
}

fn parse_recipe_head(head: &str) -> Recipe {
    let parts: Vec<&str> = head.split("  \u{b7}  ").map(str::trim).collect();
    let question = parts.first().copied().unwrap_or("").to_string();
    let mut uses = 1u32;
    let mut date = String::new();
    let mut tables: Vec<String> = Vec::new();
    for meta in parts.iter().skip(1) {
        let m = meta.trim_end_matches("  (stale: a table is gone)").trim();
        if let Some(n) = m.strip_prefix("used ").and_then(|x| x.trim_end_matches('\u{d7}').parse().ok())
        {
            uses = n;
        } else if m.len() == 10 && m.as_bytes().get(4) == Some(&b'-') {
            date = m.to_string();
        } else if !m.is_empty() {
            tables = m.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        }
    }
    Recipe {
        question,
        sql: String::new(),
        tables,
        uses,
        date: if date.is_empty() { today_ymd() } else { date },
        stale: head.contains("(stale:"),
    }
}

/// Lowercase, collapse whitespace, drop a leading `in <file>,` and trailing
/// punctuation so "In txns_00.csv, what did I spend on rent?" and "what did i
/// spend on rent" match.
fn normalise_question(q: &str) -> String {
    let mut s = q.trim().to_lowercase();
    if let Some(rest) = s.strip_prefix("in ") {
        if let Some((_, tail)) = rest.split_once(", ") {
            s = tail.to_string();
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ").trim_end_matches(['?', '.', '!']).to_string()
}

fn today_ymd() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    ymd_from_epoch(secs)
}

/// Civil date (`YYYY-MM-DD`) from Unix seconds Howard Hinnant's algorithm.
fn ymd_from_epoch(secs: u64) -> String {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_to_civil_date() {
        assert_eq!(ymd_from_epoch(0), "1970-01-01");
        assert_eq!(ymd_from_epoch(1_700_000_000), "2023-11-14");
        assert_eq!(ymd_from_epoch(1_600_000_000), "2020-09-13");
    }

    #[test]
    fn round_trips_and_supersedes() {
        let dir = std::env::temp_dir().join(format!("fella-mem-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("t.md");
        let _ = std::fs::remove_file(&path);

        let mut m = FolderMemory::load(&path);
        m.preferences.push("amounts are GBP".into());
        m.set_vocab("rent", "category IN ('rent','housing')");
        m.set_table_note("ledger.\"Amount Paid\"", "text column; cast before SUM");
        m.record_recipe(
            "in txns_00.csv, what did I spend on rent?",
            "SELECT SUM(x)\nFROM ledger",
            &["ledger".into()],
        );
        m.save();

        // reload: everything survives
        let mut m2 = FolderMemory::load(&path);
        assert_eq!(m2.preferences, vec!["amounts are GBP"]);
        assert_eq!(m2.vocabulary.len(), 1);
        assert_eq!(m2.vocabulary[0].key, "rent");
        assert_eq!(m2.tables.len(), 1);
        assert_eq!(m2.recipes.len(), 1);
        assert_eq!(m2.recipes[0].sql, "SELECT SUM(x) FROM ledger");

        // same question, different phrasing -> use-count up, not a duplicate
        m2.record_recipe("What did I spend on rent", "SELECT SUM(y) FROM ledger", &["ledger".into()]);
        assert_eq!(m2.recipes.len(), 1);
        assert_eq!(m2.recipes[0].uses, 2);

        // superseding a vocab key replaces the text
        m2.set_vocab("RENT", "category = 'rent'");
        assert_eq!(m2.vocabulary.len(), 1);
        assert_eq!(m2.vocabulary[0].text, "category = 'rent'");

        // stale marking
        m2.mark_stale(&["other_table".into()]);
        assert!(m2.recipes[0].stale);
        m2.mark_stale(&["ledger".into()]);
        assert!(!m2.recipes[0].stale);

        let core = m2.semantic_core().unwrap();
        assert!(core.contains("amounts are GBP"));
        assert!(core.contains("rent \u{2192} category = 'rent'"));
        assert!(core.contains("verified before"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preserves_user_notes_and_unknown_sections() {
        let dir = std::env::temp_dir().join(format!("fella-mem-pu-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("t.md");
        std::fs::write(
            &path,
            "# x\n\n## Preferences\n- a\n\n## Notes\nmy own words\nkeep me\n\n## Weird\nverbatim\n",
        )
        .unwrap();
        let m = FolderMemory::load(&path);
        assert_eq!(m.preferences, vec!["a"]);
        assert!(m.freeform.iter().any(|l| l == "my own words"));
        assert!(m.trailer.iter().any(|l| l == "verbatim"));
        m.save();
        let again = std::fs::read_to_string(&path).unwrap();
        assert!(again.contains("my own words"));
        assert!(again.contains("## Weird"));
        assert!(again.contains("verbatim"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn correction_markers() {
        assert!(is_correction("no, gym is under health"));
        assert!(is_correction("Actually it's category = 'health'"));
        assert!(is_correction("that's wrong  rent excludes deposits"));
        assert!(!is_correction("how much did I spend on gym?"));
        assert!(!is_correction("now show me March"));
    }
}
