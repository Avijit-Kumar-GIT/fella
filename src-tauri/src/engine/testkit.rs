//! Deterministic fixtures + known-correct ("golden") answers for the agent
//! evaluation harness (`examples/agent_eval`). Nothing here is used by the
//! shipped app it compiles only under `cfg(test)` or `--features eval`.
//!
//! Every generator is a fixed seed through a xorshift PRNG, so the golden
//! aggregates are computed in Rust as the rows are written no database round
//! trip and are exact.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

/// The 6 spend categories every synthetic transactions table uses.
pub const CATEGORIES: [&str; 6] = [
    "groceries",
    "rent",
    "transport",
    "dining",
    "utilities",
    "shopping",
];

const MERCHANTS: [&str; 16] = [
    "Aldi", "Tesco", "Uber", "Shell", "Amazon", "Netflix", "Spotify", "EDF", "Thameslink", "Pret",
    "Nando's", "IKEA", "Boots", "Costa", "Greggs", "Deliveroo",
];

/// A small deterministic xorshift64. Same recurrence `agent_bench.rs` uses.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Never seed 0 (xorshift's fixed point).
        Rng(seed | 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// Known-correct values for one generated transactions table.
#[derive(Debug, Clone, Default)]
pub struct TableGold {
    pub rows: usize,
    pub total: f64,
    /// spend per category, descending is `top3()`
    pub by_category: BTreeMap<String, f64>,
    /// (merchant, spend) for the single biggest-spend merchant
    pub top_merchant: (String, f64),
}

impl TableGold {
    /// The three highest-spend categories, `(name, amount)`, descending.
    pub fn top3_categories(&self) -> Vec<(String, f64)> {
        let mut v: Vec<_> = self
            .by_category
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v.truncate(3);
        v
    }
}

/// How rough the generated data is. 0 = clean numbers + ISO dates. Higher
/// values layer on the traps a real folder has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Messiness {
    /// numeric `amount`, ISO `date`
    Clean,
    /// `amount` written as `"$1,234.56"` text (the `parse_num` trap)
    TextAmounts,
    /// + a trailing `TOTAL` summary row that a naive `SUM` double-counts
    TotalsRow,
    /// + mixed date formats in a text `date` column
    MixedDates,
}

impl Messiness {
    fn text_amounts(self) -> bool {
        !matches!(self, Messiness::Clean)
    }
    fn totals_row(self) -> bool {
        matches!(self, Messiness::TotalsRow | Messiness::MixedDates)
    }
    fn mixed_dates(self) -> bool {
        matches!(self, Messiness::MixedDates)
    }
}

/// One synthetic workspace: `n_tables` transactions CSVs of `rows_per_table`
/// rows each, plus the two notes files.
#[derive(Debug, Clone)]
pub struct WorkspaceSpec {
    pub n_tables: usize,
    pub rows_per_table: usize,
    pub messiness: Messiness,
    pub seed: u64,
}

impl WorkspaceSpec {
    pub fn small_clean() -> Self {
        WorkspaceSpec { n_tables: 1, rows_per_table: 6_000, messiness: Messiness::Clean, seed: 1 }
    }
}

/// Golden answers for a whole generated workspace.
#[derive(Debug, Clone, Default)]
pub struct Goldens {
    /// keyed by table file stem, e.g. `"txns_00"`
    pub tables: BTreeMap<String, TableGold>,
    /// notes-budget.md: (rent target, month it changed)
    pub rent_target: f64,
    pub rent_changed: &'static str,
}

impl Goldens {
    /// Sum over every generated table useful for a "total across the folder"
    /// question.
    pub fn grand_total(&self) -> f64 {
        self.tables.values().map(|t| t.total).sum()
    }
    pub fn grand_rows(&self) -> usize {
        self.tables.values().map(|t| t.rows).sum()
    }
}

fn amount_cell(a: f64, text: bool) -> String {
    if text {
        // "$1,234.56" with a thousands separator
        let whole = a.trunc() as i64;
        let frac = ((a.fract() * 100.0).round()) as i64;
        let mut s = String::new();
        let ws = whole.abs().to_string();
        for (i, c) in ws.chars().enumerate() {
            if i > 0 && (ws.len() - i).is_multiple_of(3) {
                s.push(',');
            }
            s.push(c);
        }
        format!("\"${s}.{frac:02}\"")
    } else {
        format!("{a:.2}")
    }
}

fn date_cell(rng: &mut Rng, y: i32, m: i32, d: i32, mixed: bool) -> String {
    if !mixed {
        return format!("{y:04}-{m:02}-{d:02}");
    }
    match rng.below(3) {
        0 => format!("{y:04}-{m:02}-{d:02}"),
        1 => format!("{m:02}/{d:02}/{y:04}"),
        _ => {
            const MON: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            format!("{} {d} {y:04}", MON[(m as usize - 1).min(11)])
        }
    }
}

/// Write one transactions CSV and return its golden aggregates.
fn gen_table(path: &Path, rows: usize, spec: &WorkspaceSpec, table_seed: u64) -> TableGold {
    let mut rng = Rng::new(spec.seed ^ table_seed);
    let text_amt = spec.messiness.text_amounts();
    let mut s = String::with_capacity(rows * 40);
    s.push_str("date,amount,category,merchant\n");

    let mut gold = TableGold::default();
    let mut per_merchant: BTreeMap<String, f64> = BTreeMap::new();

    for i in 0..rows {
        let day = (i as i64) % 1400;
        let year = 2021 + (day / 365) as i32;
        let doy = (day % 365) + 1;
        let month = ((doy / 31) + 1) as i32;
        let dom = ((doy % 28) + 1) as i32;
        let cat = CATEGORIES[rng.below(CATEGORIES.len() as u64) as usize];
        let merch = MERCHANTS[rng.below(MERCHANTS.len() as u64) as usize];
        // 3.00 .. 243.00, two decimals
        let amount = 3.0 + rng.below(24_000) as f64 / 100.0;

        gold.rows += 1;
        gold.total += amount;
        *gold.by_category.entry(cat.to_string()).or_default() += amount;
        *per_merchant.entry(merch.to_string()).or_default() += amount;

        s.push_str(&date_cell(&mut rng, year, month, dom, spec.messiness.mixed_dates()));
        s.push(',');
        s.push_str(&amount_cell(amount, text_amt));
        s.push_str(&format!(",{cat},{merch}\n"));
    }

    // Round the golden aggregates the way an answer would be read back.
    gold.total = (gold.total * 100.0).round() / 100.0;
    for v in gold.by_category.values_mut() {
        *v = (*v * 100.0).round() / 100.0;
    }
    gold.top_merchant = per_merchant
        .into_iter()
        .map(|(m, v)| (m, (v * 100.0).round() / 100.0))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or_default();

    if spec.messiness.totals_row() {
        // A summary row the model must exclude: real rows sum to `gold.total`.
        s.push_str("TOTAL,");
        s.push_str(&amount_cell(gold.total, text_amt));
        s.push_str(",TOTAL,\n");
    }

    std::fs::write(path, s).unwrap();
    gold
}

/// Materialise a synthetic workspace under `dir` and return its goldens.
pub fn synth_workspace(dir: &Path, spec: &WorkspaceSpec) -> Goldens {
    std::fs::create_dir_all(dir).unwrap();
    let mut goldens = Goldens { rent_target: 1250.0, rent_changed: "March 2024", ..Default::default() };

    for t in 0..spec.n_tables {
        let stem = format!("txns_{t:02}");
        let path = dir.join(format!("{stem}.csv"));
        let g = gen_table(&path, spec.rows_per_table, spec, 0x1000 + t as u64);
        goldens.tables.insert(stem, g);
    }

    std::fs::write(
        dir.join("notes-budget.md"),
        "# Budget notes\n\nMonthly rent target is 1250. Groceries budget is 500/mo.\n\
         In March 2024 the landlord raised the rent target to 1250 from 1200.\n\
         Utilities average 140.\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("notes-2024.md"),
        "# 2024\n\nSwitched gym in Feb. Annual insurance paid in April (890).\n\
         Biggest single expense: flights in July.\n",
    )
    .unwrap();

    goldens
}

/// The hand-typed 5-row rent ledger `agent_bench.rs` uses amounts as text with
/// currency signs, the classic `parse_num` trap. Its real total is 6100.00.
pub fn write_rent_fixture(dir: &Path) -> f64 {
    std::fs::create_dir_all(dir).unwrap();
    let mut f = std::fs::File::create(dir.join("rent.csv")).unwrap();
    f.write_all(
        b"month,amount paid,method\n\
          2024-01,\"$1,200.00\",ACH\n\
          2024-02,\"1,200\",ACH\n\
          2024-03,1200,check\n\
          2024-04,\"$1,250.00\",ACH\n\
          2024-05,\"$1,250.00\",check\n",
    )
    .unwrap();
    6100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_is_deterministic_and_goldens_match_the_file() {
        let dir = std::env::temp_dir().join(format!(
            "fella-testkit-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let spec = WorkspaceSpec {
            n_tables: 2,
            rows_per_table: 500,
            messiness: Messiness::Clean,
            seed: 42,
        };
        let g1 = synth_workspace(&dir, &spec);
        let g2 = synth_workspace(&dir, &spec); // same seed -> same numbers

        // deterministic
        assert_eq!(g1.grand_rows(), 1000);
        assert_eq!(g1.grand_total(), g2.grand_total());
        assert_eq!(g1.tables["txns_00"].total, g2.tables["txns_00"].total);

        // the golden total actually equals the sum of the written `amount` column
        let csv = std::fs::read_to_string(dir.join("txns_00.csv")).unwrap();
        let file_total: f64 = csv
            .lines()
            .skip(1)
            .filter_map(|l| l.split(',').nth(1))
            .filter_map(|a| a.trim_matches(['"', '$']).replace(',', "").parse::<f64>().ok())
            .sum();
        assert!(
            (file_total - g1.tables["txns_00"].total).abs() < 0.02,
            "golden {} vs file {file_total}",
            g1.tables["txns_00"].total
        );

        // by-category sums to the total; top3 is descending
        let cat_sum: f64 = g1.tables["txns_00"].by_category.values().sum();
        assert!((cat_sum - g1.tables["txns_00"].total).abs() < 0.05);
        let top3 = g1.tables["txns_00"].top3_categories();
        assert_eq!(top3.len(), 3);
        assert!(top3[0].1 >= top3[1].1 && top3[1].1 >= top3[2].1);

        // messiness=TotalsRow appends a summary row that is NOT counted in gold
        let messy = synth_workspace(
            &dir,
            &WorkspaceSpec { messiness: Messiness::TotalsRow, ..spec.clone() },
        );
        let messy_csv = std::fs::read_to_string(dir.join("txns_00.csv")).unwrap();
        assert!(messy_csv.lines().last().unwrap().starts_with("TOTAL,"));
        assert_eq!(messy.tables["txns_00"].rows, 500, "the TOTAL row is not a data row");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rent_fixture_total_is_6100() {
        let dir = std::env::temp_dir().join(format!(
            "fella-rent-{}",
            std::process::id()
        ));
        assert_eq!(write_rent_fixture(&dir), 6100.0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
