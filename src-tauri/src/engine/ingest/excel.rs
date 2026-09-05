//! Excel ingest: `calamine` reads each sheet, we infer simple column types and
//! hand the rows to the data engine's bulk loader. Chosen over DuckDB's `excel`
//! extension because that autoloads from the network and Fella works offline.

use std::collections::HashSet;

use calamine::{open_workbook_auto, Data, Reader};
use serde_json::Value as Json;

use crate::engine::catalog::{self, ColumnInfo};
use crate::engine::data::{ColType, DataEngine};
use crate::engine::error::{EngineError, EngineResult};

/// One sheet that became a queryable table.
pub struct SheetIngest {
    pub sheet: String,
    pub view: String,
    pub row_count: i64,
    pub columns: Vec<ColumnInfo>,
    /// Sheet-level ingest caveat: preamble rows skipped above the header, a
    /// trailing totals row dropped. `None` when the sheet was clean.
    pub note: Option<String>,
}

/// Load every non-empty sheet of `path` into the engine. One failing sheet is
/// logged and skipped, not fatal a human-readable reason is returned for
/// every sheet that didn't make it into the first element, so a caller can
/// tell a user why (rather than a bare "no readable sheets" that throws the
/// real cause away).
pub fn ingest_workbook(
    engine: &mut dyn DataEngine,
    path: &str,
    stem: &str,
    used: &mut HashSet<String>,
) -> EngineResult<(Vec<SheetIngest>, Vec<String>)> {
    let mut workbook =
        open_workbook_auto(path).map_err(|e| EngineError::msg(format!("open {path}: {e}")))?;

    let sheet_names = workbook.sheet_names().to_vec();
    let multi = sheet_names.len() > 1;
    let mut out = Vec::new();
    let mut skip_reasons: Vec<String> = Vec::new();

    for sheet in sheet_names {
        let range = match workbook.worksheet_range(&sheet) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("{path} [{sheet}]: {e}");
                skip_reasons.push(format!("{sheet}: couldn't read the sheet ({e})"));
                continue;
            }
        };
        if range.is_empty() || range.height() == 0 || range.width() == 0 {
            skip_reasons.push(format!("{sheet}: empty sheet"));
            continue;
        }

        let rows: Vec<&[Data]> = range.rows().collect();
        let (headers, data_start) = header_row(&rows);
        let mut data: &[&[Data]] = &rows[data_start..];
        if data.is_empty() {
            skip_reasons.push(format!("{sheet}: no data rows after the header"));
            continue;
        }

        // Drop a trailing "Total" / "Sum" summary row so it can't double-count.
        let mut dropped_total = false;
        if data.len() >= 3 {
            if let Some((last, rest)) = data.split_last() {
                if looks_like_total_row(last, headers.len()) {
                    data = rest;
                    dropped_total = true;
                }
            }
        }

        let inferred = infer_columns(data, headers.len());
        let types: Vec<ColType> = inferred.iter().map(|c| c.ty).collect();
        let view = if multi {
            catalog::unique_view_name(&format!("{stem}_{sheet}"), used)
        } else {
            catalog::unique_view_name(stem, used)
        };

        let cols: Vec<(String, ColType)> =
            headers.iter().cloned().zip(types.iter().copied()).collect();
        let matrix: Vec<Vec<Json>> = data
            .iter()
            .map(|row| {
                (0..headers.len())
                    .map(|i| cell_to_json(row.get(i).unwrap_or(&Data::Empty), types[i]))
                    .collect()
            })
            .collect();

        let mut sheet_notes: Vec<String> = Vec::new();
        if data_start > 1 {
            sheet_notes.push(format!(
                "{} row(s) above the header were skipped as preamble",
                data_start - 1
            ));
        }
        if dropped_total {
            sheet_notes.push("a trailing total/summary row was excluded".into());
        }

        match engine.add_rows(&view, &cols, &matrix) {
            Ok(n) => out.push(SheetIngest {
                sheet,
                columns: cols
                    .iter()
                    .zip(&inferred)
                    .map(|((name, t), ci)| {
                        let mut c = ColumnInfo::bare(name.clone(), t.sqlite());
                        c.note = ci.note.clone();
                        c
                    })
                    .collect(),
                view,
                row_count: n,
                note: if sheet_notes.is_empty() {
                    None
                } else {
                    Some(sheet_notes.join("; "))
                },
            }),
            Err(e) => {
                log::warn!("{path} [{sheet}]: {e}");
                skip_reasons.push(format!("{sheet}: {e}"));
            }
        }
    }

    Ok((out, skip_reasons))
}

/// How many cells in a row carry real content (not empty, not a blank-ish
/// placeholder string).
fn filled(row: &[Data]) -> usize {
    row.iter()
        .filter(|c| match c {
            Data::Empty => false,
            Data::String(s) => !crate::engine::data::is_blankish(s),
            _ => true,
        })
        .count()
}

fn names_from(row: &[Data], width: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    (0..width)
        .map(|i| {
            let raw = match row.get(i) {
                Some(Data::String(s)) => s.trim().to_string(),
                _ => String::new(),
            };
            let base = if raw.is_empty() { format!("col{}", i + 1) } else { raw };
            let mut name = base.clone();
            let mut n = 2;
            while !seen.insert(name.clone()) {
                name = format!("{base}_{n}");
                n += 1;
            }
            name
        })
        .collect()
}

/// Pick the header row, skipping banner/title and blank spacer rows that
/// human-made spreadsheets often carry above the real headers. Returns the
/// header names and the index of the first data row.
fn header_row(rows: &[&[Data]]) -> (Vec<String>, usize) {
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);

    let mut skipped_any = false;
    // Scan a good way in some exported reports carry a dozen preamble rows
    // (logo, title, date range, filters) above the real header.
    for i in 0..rows.len().min(15) {
        if i + 1 >= rows.len() {
            break;
        }
        let r = rows[i];
        let f = filled(r);
        // A header must be all text (empty cells allowed - they become `colN`).
        // Fill bar: every column named for a row-0 header (no preamble to lean
        // on), but only a majority once we've already skipped a banner/spacer,
        // so `Date | (unnamed) | Method` under a title still counts.
        let min_fill = if skipped_any { width.div_ceil(2) } else { width };
        let all_str = r.iter().take(width).all(|c| match c {
            Data::Empty => true,
            Data::String(s) => !s.trim().is_empty(),
            _ => false,
        });
        if f == 0 || f < min_fill || !all_str {
            skipped_any = true;
            continue;
        }
        return (names_from(r, width), i + 1);
    }

    // Fallback: original row-0 heuristic.
    let first = rows[0];
    let looks_like_header = rows.len() > 1
        && !first.is_empty()
        && first
            .iter()
            .all(|c| matches!(c, Data::String(s) if !s.trim().is_empty()));
    if looks_like_header {
        (names_from(first, width), 1)
    } else {
        ((0..width).map(|i| format!("col{}", i + 1)).collect(), 0)
    }
}

/// A trailing row whose only text cell is "total" / "sum" / "grand total" and
/// which is otherwise mostly empty a summary line, not data.
fn looks_like_total_row(row: &[Data], width: usize) -> bool {
    let label = row.iter().find_map(|c| match c {
        Data::String(s) if !s.trim().is_empty() => Some(s.trim().to_ascii_lowercase()),
        _ => None,
    });
    let is_total = match label.as_deref() {
        Some(l) => {
            let l = l.trim_end_matches([':', '.']).trim();
            matches!(l, "total" | "totals" | "sum" | "grand total" | "subtotal" | "sub total")
                || l.starts_with("total ")
                || l.starts_with("grand total ")
                || l.starts_with("subtotal ")
        }
        None => false,
    };
    // A summary line is label + a figure or two, not a full data row.
    is_total && filled(row) * 3 <= width * 2 + 2
}

struct ColInfer {
    ty: ColType,
    note: Option<String>,
}

/// Per-column type inference over the data rows. Blank-ish cells are ignored;
/// a column that isn't natively numeric but whose remaining cells are all
/// written numbers (`$1,200`, `1,150`) becomes `Float` with a note.
fn infer_columns(data: &[&[Data]], width: usize) -> Vec<ColInfer> {
    use crate::engine::data::{is_blankish, parse_numeric};

    (0..width)
        .map(|i| {
            let (mut nonblank, mut numeric, mut booleans, mut others) = (0usize, 0usize, 0usize, 0usize);
            let (mut all_integral, mut saw_coerced) = (true, false);

            for row in data {
                match row.get(i) {
                    None | Some(Data::Empty) => continue,
                    Some(Data::String(s)) if is_blankish(s) => continue,
                    Some(Data::Int(_)) => {
                        nonblank += 1;
                        numeric += 1;
                    }
                    Some(Data::Float(f)) => {
                        nonblank += 1;
                        numeric += 1;
                        if f.fract() != 0.0 {
                            all_integral = false;
                        }
                    }
                    Some(Data::Bool(_)) => {
                        nonblank += 1;
                        booleans += 1;
                    }
                    Some(Data::String(s)) => {
                        nonblank += 1;
                        match parse_numeric(s) {
                            Some(v) => {
                                numeric += 1;
                                saw_coerced = true;
                                if v.fract() != 0.0 {
                                    all_integral = false;
                                }
                            }
                            None => others += 1,
                        }
                    }
                    Some(_) => {
                        nonblank += 1;
                        others += 1;
                    }
                }
            }

            let coerce_note = || {
                Some("amounts were stored as text (currency, commas, percent) and read as numbers".to_string())
            };

            if nonblank == 0 {
                return ColInfer { ty: ColType::Text, note: None };
            }
            if booleans == nonblank {
                return ColInfer { ty: ColType::Bool, note: None };
            }
            if numeric > 0 && numeric == nonblank {
                if saw_coerced {
                    return ColInfer { ty: ColType::Float, note: coerce_note() };
                }
                let ty = if all_integral { ColType::Int } else { ColType::Float };
                return ColInfer { ty, note: None };
            }
            // A few unparseable stragglers in an otherwise-numeric column: coerce
            // the column and null the stragglers, but say so.
            if numeric >= 3 && (others as f64) / (nonblank as f64) <= 0.05 {
                return ColInfer {
                    ty: ColType::Float,
                    note: Some(format!(
                        "{others} value(s) could not be read as a number and were left blank"
                    )),
                };
            }
            if numeric * 2 > nonblank {
                return ColInfer {
                    ty: ColType::Text,
                    note: Some("looks numeric but is mixed; kept as text - CAST it for a total".into()),
                };
            }
            ColInfer { ty: ColType::Text, note: None }
        })
        .collect()
}

fn cell_to_json(cell: &Data, ty: ColType) -> Json {
    use crate::engine::data::{is_blankish, parse_numeric};

    if let Data::Empty = cell {
        return Json::Null;
    }
    if let Data::String(s) = cell {
        // Blank-ish placeholders are missing data in a numeric/bool column, but
        // a real value in a text column (kept verbatim); an empty string is
        // NULL everywhere.
        if s.trim().is_empty() || (is_blankish(s) && ty != ColType::Text) {
            return Json::Null;
        }
    }
    match (cell, ty) {
        (Data::Int(n), ColType::Int) => Json::from(*n),
        (Data::Int(n), ColType::Float) => Json::from(*n as f64),
        (Data::Float(f), ColType::Int) => Json::from(*f as i64),
        (Data::Float(f), ColType::Float) => {
            serde_json::Number::from_f64(*f).map(Json::Number).unwrap_or(Json::Null)
        }
        (Data::Bool(b), ColType::Bool) => Json::Bool(*b),
        (Data::String(s), ColType::Float) => parse_numeric(s)
            .and_then(serde_json::Number::from_f64)
            .map(Json::Number)
            .unwrap_or(Json::Null),
        (Data::String(s), ColType::Int) => {
            parse_numeric(s).map(|v| Json::from(v as i64)).unwrap_or(Json::Null)
        }
        (_, ColType::Text) => Json::from(cell_to_string(cell)),
        _ => Json::Null,
    }
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Int(n) => n.to_string(),
        Data::Float(f) => f.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::DateTime(dt) => excel_serial_to_iso(dt.as_f64()),
        Data::Error(e) => format!("#ERR:{e:?}"),
    }
}

/// Excel serial date (days since 1899-12-30) → ISO-8601. Dependency-free.
fn excel_serial_to_iso(serial: f64) -> String {
    let unix_secs = (serial - 25_569.0) * 86_400.0;
    let secs = unix_secs.floor() as i64;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    if serial.fract() == 0.0 {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        let (hh, mm, ss) = (tod / 3600, (tod % 3600) / 60, tod % 60);
        format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
    }
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_dates() {
        assert_eq!(excel_serial_to_iso(45000.0), "2023-03-15");
        assert_eq!(excel_serial_to_iso(45000.5), "2023-03-15 12:00:00");
        assert_eq!(excel_serial_to_iso(44927.0), "2023-01-01");
    }

    #[test]
    fn infers_column_types() {
        let r1 = [Data::Int(1), Data::Float(1.5), Data::Bool(true), Data::String("x".into())];
        let r2 = [Data::Int(2), Data::Int(3), Data::Bool(false), Data::Empty];
        let rows: Vec<&[Data]> = vec![&r1, &r2];
        let t = infer_columns(&rows, 4);
        assert_eq!(t[0].ty, ColType::Int);
        assert_eq!(t[1].ty, ColType::Float);
        assert_eq!(t[2].ty, ColType::Bool);
        assert_eq!(t[3].ty, ColType::Text);
    }

    #[test]
    fn coerces_currency_text_column() {
        let r1 = [Data::String("$1,200.00".into())];
        let r2 = [Data::String("1,150".into())];
        let r3 = [Data::String("1200".into())];
        let r4 = [Data::String("N/A".into())];
        let rows: Vec<&[Data]> = vec![&r1, &r2, &r3, &r4];
        let c = infer_columns(&rows, 1);
        assert_eq!(c[0].ty, ColType::Float, "currency text should coerce to Float");
        assert!(c[0].note.is_some());
        assert_eq!(cell_to_json(&Data::String("$1,200.00".into()), ColType::Float), Json::from(1200.0));
        assert_eq!(cell_to_json(&Data::String("N/A".into()), ColType::Float), Json::Null);
    }

    #[test]
    fn text_column_keeps_placeholder_cells() {
        // A "none" / "-" cell is a real value in a text column, NULL in a
        // numeric one; an empty string is NULL either way.
        assert_eq!(cell_to_json(&Data::String("none".into()), ColType::Text), Json::from("none"));
        assert_eq!(cell_to_json(&Data::String("-".into()), ColType::Text), Json::from("-"));
        assert_eq!(cell_to_json(&Data::String("  ".into()), ColType::Text), Json::Null);
        assert_eq!(cell_to_json(&Data::String("none".into()), ColType::Float), Json::Null);
    }

    #[test]
    fn header_row_skips_title_and_spacer() {
        let title = [Data::String("Rent Ledger 2024".into()), Data::Empty, Data::Empty];
        let spacer = [Data::Empty, Data::Empty, Data::Empty];
        let hdr = [
            Data::String("Date".into()),
            Data::String("Amount Paid".into()),
            Data::String("Method".into()),
        ];
        let d1 = [
            Data::String("2024-01-01".into()),
            Data::String("$1,200".into()),
            Data::String("ACH".into()),
        ];
        let rows: Vec<&[Data]> = vec![&title, &spacer, &hdr, &d1];
        let (names, start) = header_row(&rows);
        assert_eq!(names, vec!["Date", "Amount Paid", "Method"]);
        assert_eq!(start, 3, "data starts after the real header");
    }

    #[test]
    fn header_row_allows_an_unnamed_column_below_a_title() {
        let title = [Data::String("2024 spend".into()), Data::Empty, Data::Empty];
        let hdr = [Data::String("Date".into()), Data::Empty, Data::String("Method".into())];
        let d1 = [
            Data::String("2024-01-01".into()),
            Data::Float(12.0),
            Data::String("ACH".into()),
        ];
        let d2 = [
            Data::String("2024-02-01".into()),
            Data::Float(15.0),
            Data::String("ACH".into()),
        ];
        let rows: Vec<&[Data]> = vec![&title, &hdr, &d1, &d2];
        let (names, start) = header_row(&rows);
        assert_eq!(names, vec!["Date", "col2", "Method"]);
        assert_eq!(start, 2);
    }

    #[test]
    fn detects_trailing_total_row() {
        let total = [Data::String("Total".into()), Data::Float(3550.0), Data::Empty];
        assert!(looks_like_total_row(&total, 3));
        let data = [Data::String("2024-03-01".into()), Data::Float(1200.0), Data::String("ACH".into())];
        assert!(!looks_like_total_row(&data, 3));
    }
}
