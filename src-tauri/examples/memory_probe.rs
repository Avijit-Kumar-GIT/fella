//! Repeatedly exercise the embedded Python/WASM path and look for memory growth.
//!
//! This is a smoke probe for release builds, not a proof that every allocator
//! or platform API is leak-free. It measures the disposable Wasm store's peak
//! linear memory on every run and, on Linux, samples the process RSS. Run it
//! through scripts/check-memory.sh so the same guest artifact and optimized
//! profile used by a release are tested.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::EngineState;

const DEFAULT_ITERATIONS: usize = 32;
const DEFAULT_MAX_RSS_GROWTH_MB: u64 = 128;

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(target_os = "linux")]
fn process_rss_bytes() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let kb = status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
    })?;
    Some(kb * 1024)
}

#[cfg(not(target_os = "linux"))]
fn process_rss_bytes() -> Option<u64> {
    None
}

fn format_bytes(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    if bytes >= MB {
        format!("{:.1} MiB", bytes as f64 / MB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

fn write_fixture(workspace: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(workspace)?;
    let mut csv = String::from("name,amount\n");
    for i in 0..256 {
        csv.push_str(&format!("item-{i},{}\n", i * 3));
    }
    fs::write(workspace.join("sales.csv"), csv)?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let iterations = env_usize("FELLA_MEMORY_ITERATIONS", DEFAULT_ITERATIONS);
    let max_growth_mb = env_u64("FELLA_MEMORY_MAX_GROWTH_MB", DEFAULT_MAX_RSS_GROWTH_MB);
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root =
        std::env::temp_dir().join(format!("fella-memory-probe-{}-{stamp}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let data_dir = root.join("data");
    let workspace = root.join("workspace");
    write_fixture(&workspace)?;

    let engine = EngineState::new(&data_dir)?;
    engine.open_workspace(&workspace)?;

    // Warm the module and allocator before taking the baseline. The probe is
    // interested in growth after a normal first use, rather than one-time
    // runtime initialization.
    let warmup = engine
        .run_python("rows = sql('SELECT sum(amount) AS total FROM sales')\nprint(rows[0]['total'])")
        .await?;
    if warmup.exit_code != Some(0) || warmup.timed_out || warmup.cancelled {
        return Err(format!("Python warm-up failed: {:?}", warmup.stderr).into());
    }

    let baseline_rss = process_rss_bytes();
    let verbose = std::env::var_os("FELLA_MEMORY_VERBOSE").is_some();
    let mut peak_guest = warmup.guest_memory_bytes as u64;
    let mut peak_rss = baseline_rss;
    let mut last_rss = baseline_rss;

    for index in 0..iterations {
        let code = match index % 3 {
            0 => "rows = sql('SELECT sum(amount) AS total FROM sales')\nprint(rows[0]['total'])",
            1 => "values = [i * 3 for i in range(1000)]\nprint(sum(values))",
            _ => "print(median([1, 4, 2, 9, 7]))\nprint(stdev([1, 2, 3, 4]))",
        };
        let result = engine
            .run_python(code)
            .await
            .map_err(|error| format!("Python probe iteration {index} failed: {error}"))?;
        if result.exit_code != Some(0) || result.timed_out || result.cancelled {
            return Err(
                format!("Python probe iteration {index} failed: {:?}", result.stderr).into(),
            );
        }
        peak_guest = peak_guest.max(result.guest_memory_bytes as u64);
        if let Some(rss) = process_rss_bytes() {
            peak_rss = Some(peak_rss.unwrap_or(rss).max(rss));
            last_rss = Some(rss);
            if verbose {
                println!(
                    "iteration {index}: guest {}, RSS {}",
                    format_bytes(result.guest_memory_bytes as u64),
                    format_bytes(rss)
                );
            }
        } else if verbose {
            println!(
                "iteration {index}: guest {}, RSS unavailable",
                format_bytes(result.guest_memory_bytes as u64)
            );
        }
    }

    println!("memory probe: {iterations} post-warm-up iterations");
    println!("guest Wasm memory peak: {}", format_bytes(peak_guest));
    match (baseline_rss, last_rss, peak_rss) {
        (Some(start), Some(end), Some(peak)) => {
            let growth = end.saturating_sub(start);
            println!(
                "Linux process RSS: start {}, end {}, peak {}, end growth {}",
                format_bytes(start),
                format_bytes(end),
                format_bytes(peak),
                format_bytes(growth)
            );
            let max_growth = max_growth_mb * 1024 * 1024;
            if growth > max_growth {
                return Err(format!(
                    "RSS grew by {} after warm-up; configured limit is {}",
                    format_bytes(growth),
                    format_bytes(max_growth)
                )
                .into());
            }
        }
        _ => println!("process RSS is unavailable on this platform; guest memory was measured"),
    }
    println!("memory probe passed");
    Ok(())
}
