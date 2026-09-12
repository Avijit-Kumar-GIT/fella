//! Per-folder learned notes: a plain-Markdown file Fella keeps for a workspace,
//! written from what the user *corrects*, and that the user can read and edit.
//! Not a memory system it's the learned sibling of `fella.md`.
//!
//! One `.md` per workspace under `<data_dir>/memory/`, plus an append-only
//! `.episodes.jsonl` (the raw record; not yet read back). The `.md` is the
//! source of truth: parsing is lenient (an unrecognised line inside a managed
//! section is kept verbatim), and a user-authored `## Notes` section and any
//! sections we don't know are preserved on rewrite.
//!
//! Deliberately stores only durable *facts* (preferences, vocabulary, table
//! notes) never a cached *query*. An earlier version also cached
//! `question -> SQL` "recipes" reused whenever a later question looked
//! similar enough. Cut (2026-09-11, see `docs/DECISIONS.md`): a recipe is
//! code, not a fact, and it's recorded the moment `verify` happens to pass —
//! so a query that only *looked* right (or that a since-strengthened check
//! would now catch) got frozen in and replayed verbatim, which is a stronger
//! claim than "this fact holds," and it discourages the fresh reasoning that
//! would otherwise re-derive the right query for a subtly different
//! question. A `## Recipes` section from an older memory file is silently
//! dropped on next load.
//!
//! `FELLA_MEMORY=0` turns the whole thing off (read and write).

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Episodes kept in the `.jsonl` before the oldest are trimmed.
const MAX_EPISODES: usize = 200;

/// Reading learned notes is on unless `FELLA_MEMORY=0`.
pub fn enabled() -> bool {
    !matches!(std::env::var("FELLA_MEMORY").as_deref(), Ok("0"))
}

/// Writing (recipes, notes, episodes) is on unless `FELLA_MEMORY` is `0` or
/// `ro` `ro` reads existing notes but records nothing (used by the eval so
/// every cold run sees the same primed memory).
pub fn writes_enabled() -> bool {
    !matches!(std::env::var("FELLA_MEMORY").as_deref(), Ok("0") | Ok("ro"))
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

#[derive(Default)]
pub struct FolderMemory {
    path: PathBuf,
    preferences: Vec<String>,
    vocabulary: Vec<Note>,
    tables: Vec<Note>,
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
    /// A no-longer-supported section (just `## Recipes`) silently discarded
    /// on load rather than preserved as a `Trailer` an old memory file
    /// self-cleans the moment Fella next touches it.
    Dropped,
    Freeform,
    Trailer,
}

impl FolderMemory {
    /// Load from disk; an absent file is an empty memory.
    pub fn load(path: &Path) -> Self {
        let mut m = FolderMemory { path: path.to_path_buf(), ..Default::default() };
        let Ok(text) = std::fs::read_to_string(path) else { return m };
        let mut sec = Section::None;
        for raw in text.lines() {
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
                    "notes" => Section::Freeform,
                    "recipes" => Section::Dropped,
                    _ => {
                        m.trailer.push(raw.to_string());
                        Section::Trailer
                    }
                };
                continue;
            }
            match sec {
                Section::None | Section::Dropped => {} // H1/comments/blank, or a discarded old section
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
            "<!-- Fella writes this from corrections you give it.\n     \
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
            && self.freeform.iter().all(|l| l.trim().is_empty())
            && self.trailer.is_empty()
    }

    /// Drop table notes whose view is no longer in the workspace (a renamed or
    /// removed file). The key is `view` or `view."col"`; the view is the part
    /// before the first `.` or `."`.
    pub fn prune_tables(&mut self, known_views: &[String]) {
        self.tables.retain(|n| {
            let view = n.key.split(['.', '"']).next().unwrap_or(&n.key).trim();
            view.is_empty() || known_views.iter().any(|k| k == view)
        });
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

    /// Current vocabulary as `(key, text)` pairs -- for deciding whether a new
    /// correction updates one of these or is genuinely new. Cloned; the list
    /// stays small (a handful to dozens of entries over a folder's life).
    pub fn vocabulary_entries(&self) -> Vec<(String, String)> {
        self.vocabulary.iter().map(|n| (n.key.clone(), n.text.clone())).collect()
    }

    /// The block prepended to the system prompt: preferences, all vocabulary,
    /// and all table notes. `None` if there's nothing.
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
        if !self.tables.is_empty() {
            p.push_str("Table notes:\n");
            for n in &self.tables {
                if n.key.is_empty() {
                    p.push_str(&format!("- {}\n", n.text));
                } else {
                    p.push_str(&format!("- {}: {}\n", n.key, n.text));
                }
            }
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


#[cfg(test)]
mod tests {
    use super::*;

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
        m.save();

        // reload: everything survives
        let mut m2 = FolderMemory::load(&path);
        assert_eq!(m2.preferences, vec!["amounts are GBP"]);
        assert_eq!(m2.vocabulary.len(), 1);
        assert_eq!(m2.vocabulary[0].key, "rent");
        assert_eq!(m2.tables.len(), 1);

        // superseding a vocab key replaces the text
        m2.set_vocab("RENT", "category = 'rent'");
        assert_eq!(m2.vocabulary.len(), 1);
        assert_eq!(m2.vocabulary[0].text, "category = 'rent'");

        // pruning table notes for a view that's gone
        m2.set_table_note("gone_view.\"x\"", "note");
        assert_eq!(m2.tables.len(), 2);
        m2.prune_tables(&["ledger".into()]);
        assert_eq!(m2.tables.len(), 1);
        assert!(m2.tables[0].key.starts_with("ledger"));

        let core = m2.semantic_core().unwrap();
        assert!(core.contains("amounts are GBP"));
        assert!(core.contains("rent \u{2192} category = 'rent'"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An old memory file with a `## Recipes` section (from before recipes were
    /// cut) loads cleanly, drops it silently, and never writes it back.
    #[test]
    fn drops_a_legacy_recipes_section_on_load() {
        let dir = std::env::temp_dir().join(format!("fella-mem-legacy-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("t.md");
        std::fs::write(
            &path,
            "# x\n\n## Preferences\n- amounts are GBP\n\n## Recipes\n\
             - monthly rent  \u{b7}  used 2\u{d7}  \u{b7}  ledger  \u{b7}  2026-09-01\n    \
             SELECT strftime('%Y-%m', date) FROM ledger\n",
        )
        .unwrap();

        let m = FolderMemory::load(&path);
        assert_eq!(m.preferences, vec!["amounts are GBP"]);
        m.save();

        let again = std::fs::read_to_string(&path).unwrap();
        assert!(!again.contains("Recipes"));
        assert!(!again.contains("strftime"));

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
