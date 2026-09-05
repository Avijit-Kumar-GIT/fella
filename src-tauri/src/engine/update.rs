//! `/update`: check the GitHub releases page for a newer build and, if one
//! exists, download + checksum-verify the right installer for this OS and
//! hand off to it the same way `scripts/install.sh` / `install.ps1` already
//! do by hand, just triggered from inside the running app.
//!
//! Manual only there is no background/startup check. Fella makes this one
//! extra network call (`api.github.com`) only when the user types `/update`;
//! see `docs/SECURITY-REVIEW-v0.1.md`'s egress map.
//!
//! Applying an update means replacing the binary that's currently running,
//! which every OS restricts differently (Windows won't let anything
//! overwrite a running .exe; Linux and macOS are more permissive). The
//! version-check / download / checksum logic below is plain, testable code;
//! the per-OS "apply" step at the bottom shells out to the same tools a
//! person would use by hand (`msiexec`, `hdiutil`, a plain file replace) and
//! is inherently harder to unit-test than a testable pattern really allows
//! it needs a real install of each OS to fully verify.

use serde::{Deserialize, Serialize};

use crate::engine::error::{EngineError, EngineResult};

const REPO: &str = "Avijit-Kumar-GIT/fella";

fn latest_release_url() -> String {
    std::env::var("FELLA_RELEASE_API_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest"))
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

/// What the frontend needs to render "you're up to date" vs. an in-progress
/// update message. `available` is false once `apply()` has actually kicked
/// off the platform installer there's nothing left to report at that point
/// the app is about to exit.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub current: String,
    pub latest: String,
    pub available: bool,
}

/// Parse a plain `X.Y.Z` version (a release tag with its leading `v`
/// stripped, or `CARGO_PKG_VERSION`) into a comparable triple. `None` on
/// anything that doesn't fit the shape rather than guessing partially
/// treating an unparseable version as "not newer" is the safe default.
fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_semver(current), parse_semver(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

async fn fetch_bytes(http: &reqwest::Client, url: &str) -> EngineResult<Vec<u8>> {
    let resp = http
        .get(url)
        .header("User-Agent", "fella-app")
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| EngineError::msg(format!("could not reach {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(EngineError::msg(format!("{url}: HTTP {}", resp.status())));
    }
    Ok(resp
        .bytes()
        .await
        .map_err(|e| EngineError::msg(format!("reading {url}: {e}")))?
        .to_vec())
}

async fn fetch_json(http: &reqwest::Client, url: &str) -> EngineResult<GhRelease> {
    let resp = http
        .get(url)
        .header("User-Agent", "fella-app")
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| EngineError::msg(format!("could not reach {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(EngineError::msg(format!("{url}: HTTP {}", resp.status())));
    }
    resp.json::<GhRelease>()
        .await
        .map_err(|e| EngineError::msg(format!("reading the release list: {e}")))
}

/// Candidate asset names for this OS, most preferred first (the ones
/// `scripts/install.sh` / `install.ps1` already prefer, in the same order).
/// Exact names, not substring matching install.ps1's own history shows
/// substring guessing is the kind of thing that quietly breaks.
fn asset_candidates(version: &str) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        vec![
            format!("Fella_{version}_x64-setup.exe"),
            format!("Fella_{version}_x64_en-US.msi"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            format!("Fella_{version}_universal.dmg"),
            format!("Fella_{version}_universal.app.tar.gz"),
        ]
    }
    #[cfg(target_os = "linux")]
    {
        vec![
            format!("Fella_{version}_amd64.AppImage"),
            format!("Fella_{version}_amd64.deb"),
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = version;
        Vec::new()
    }
}

fn find_asset<'a>(assets: &'a [GhAsset], candidates: &[String]) -> Option<&'a GhAsset> {
    candidates
        .iter()
        .find_map(|name| assets.iter().find(|a| &a.name == name))
}

/// `sums_text` is `release.yml`'s `sha256sum -b * | sort -k2` output: lines
/// of `<hash> *<filename>` (or plain `<hash>  <filename>` older tools/hand
/// runs may use two spaces instead of the binary-mode `*`). Match either.
fn find_checksum(sums_text: &str, filename: &str) -> Option<String> {
    sums_text.lines().find_map(|line| {
        let line = line.trim_end_matches(['\r', '\n']);
        let rest = line.strip_suffix(filename)?;
        let hash = rest.trim_end_matches(['*', ' ', '\t']);
        // Require at least one real separator character between the hash
        // and the filename, so a line that merely *ends* with this
        // filename as a substring of something longer doesn't false-match.
        (!hash.is_empty() && hash.len() < rest.len())
            .then(|| hash.to_ascii_lowercase())
    })
}

async fn verify_checksum(
    http: &reqwest::Client,
    assets: &[GhAsset],
    filename: &str,
    bytes: &[u8],
) -> EngineResult<()> {
    let Some(sums_asset) = assets.iter().find(|a| a.name == "SHA256SUMS") else {
        // Every release since v0.1.0 carries one; a missing one is unusual
        // enough to refuse rather than install unverified, unlike the
        // install scripts' softer "loud warning" for pre-SHA256SUMS releases
        // there are none of those to be backward-compatible with here.
        return Err(EngineError::msg(
            "the latest release has no SHA256SUMS nothing installed",
        ));
    };
    let sums_text_bytes = fetch_bytes(http, &sums_asset.browser_download_url).await?;
    let sums_text = String::from_utf8_lossy(&sums_text_bytes);
    let want = find_checksum(&sums_text, filename)
        .ok_or_else(|| EngineError::msg(format!("SHA256SUMS has no entry for {filename}")))?;
    let got = sha256_hex(bytes);
    if want != got {
        return Err(EngineError::msg(format!(
            "checksum mismatch for {filename} (expected {want}, got {got}) nothing installed"
        )));
    }
    Ok(())
}

/// Check only report what's available without downloading or touching
/// anything. Used by `apply()` and exposed on its own so a caller could show
/// "a new version is available" without side effects if that's ever wanted.
pub async fn check(http: &reqwest::Client) -> EngineResult<UpdateStatus> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let release = fetch_json(http, &latest_release_url()).await?;
    let latest = release.tag_name.trim_start_matches('v').to_string();
    let available = is_newer(&current, &latest);
    Ok(UpdateStatus { current, latest, available })
}

/// Check, and if a newer version exists: download the right installer for
/// this OS, verify it against SHA256SUMS, hand off to the platform-specific
/// apply step, and exit. Returns normally (without exiting) only when
/// already up to date, or on any failure before the handoff nothing is
/// downloaded or run halfway.
pub async fn apply(http: &reqwest::Client, app: tauri::AppHandle) -> EngineResult<UpdateStatus> {
    let status = check(http).await?;
    if !status.available {
        return Ok(status);
    }
    let release = fetch_json(http, &latest_release_url()).await?;
    let candidates = asset_candidates(&status.latest);
    if candidates.is_empty() {
        return Err(EngineError::msg("no update path for this OS yet"));
    }
    let asset = find_asset(&release.assets, &candidates).ok_or_else(|| {
        EngineError::msg(format!(
            "the latest release has no installer for this platform (looked for {})",
            candidates.join(" or ")
        ))
    })?;
    let bytes = fetch_bytes(http, &asset.browser_download_url).await?;
    verify_checksum(http, &release.assets, &asset.name, &bytes).await?;

    let dir = std::env::temp_dir().join("fella-update");
    std::fs::create_dir_all(&dir)
        .map_err(|e| EngineError::io("create update staging dir", e))?;
    let staged = dir.join(&asset.name);
    std::fs::write(&staged, &bytes).map_err(|e| EngineError::io("write downloaded installer", e))?;

    platform::apply(&staged, &app)?;
    // `platform::apply` exits the process once the handoff is spawned, so
    // this normally never runs the frontend either never gets a response
    // (the process died first) or gets exactly this one. Either way,
    // `available: true` here means "found one and it's installing", not
    // "still available to install" there's no separate confirm step this
    // function always applies an update the moment it finds one.
    Ok(status)
}

#[cfg(target_os = "windows")]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    use crate::engine::error::{EngineError, EngineResult};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    /// Windows won't let the new installer overwrite the running .exe. Spawn
    /// a detached shell that waits a couple of seconds (for this process to
    /// fully exit and release the file lock), runs the installer silently,
    /// then relaunches Fella from the same path it's running from today
    /// (an upgrade installs in place, so `current_exe()` is still correct
    /// afterward) then exit immediately so the wait has something to wait
    /// for. Best-effort: not verified against a real Windows install yet.
    pub fn apply(installer: &Path, app: &tauri::AppHandle) -> EngineResult<()> {
        let exe = std::env::current_exe().map_err(|e| EngineError::io("find current exe", e))?;
        let installer_cmd = if installer.extension().and_then(|e| e.to_str()) == Some("msi") {
            format!("msiexec /i \"{}\" /passive", installer.display())
        } else {
            format!("\"{}\" /S", installer.display())
        };
        let script = format!(
            "timeout /t 2 /nobreak >nul & {installer_cmd} & \"{}\"",
            exe.display()
        );
        Command::new("cmd")
            .args(["/C", &script])
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .map_err(|e| EngineError::io("launch the installer", e))?;
        app.exit(0);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::Path;
    use std::process::Command;

    use crate::engine::error::{EngineError, EngineResult};

    /// Mirrors install.sh's macOS steps by hand: mount the dmg, replace the
    /// .app bundle at the same path this process is already running from,
    /// drop the quarantine flag, relaunch. Runs detached via a small shell
    /// script so it can outlive this process once it exits. Best-effort:
    /// not verified against a real macOS install yet.
    pub fn apply(installer: &Path, app: &tauri::AppHandle) -> EngineResult<()> {
        let exe = std::env::current_exe().map_err(|e| EngineError::io("find current exe", e))?;
        // .../Fella.app/Contents/MacOS/fella -> .../Fella.app
        let app_bundle = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .ok_or_else(|| EngineError::msg("could not locate the .app bundle to replace"))?
            .to_path_buf();
        let dest_dir = app_bundle
            .parent()
            .ok_or_else(|| EngineError::msg("could not locate the Applications folder"))?
            .to_path_buf();
        let name = app_bundle
            .file_name()
            .ok_or_else(|| EngineError::msg("could not read the app bundle name"))?
            .to_string_lossy()
            .to_string();

        let script = format!(
            r#"
sleep 2
mnt="$(mktemp -d)"
hdiutil attach -nobrowse -quiet -mountpoint "$mnt" "{installer}"
src="$(find "$mnt" -maxdepth 1 -name '*.app' -print -quit)"
rm -rf "{dest_dir}/{name}"
cp -R "$src" "{dest_dir}/"
hdiutil detach -quiet "$mnt" || true
xattr -dr com.apple.quarantine "{dest_dir}/{name}" 2>/dev/null || true
open "{dest_dir}/{name}"
"#,
            installer = installer.display(),
            dest_dir = dest_dir.display(),
            name = name,
        );
        Command::new("sh")
            .args(["-c", &script])
            .spawn()
            .map_err(|e| EngineError::io("launch the update script", e))?;
        app.exit(0);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use crate::engine::error::{EngineError, EngineResult};

    /// AppImage: replace the running image in place (Linux allows replacing
    /// a file that's currently executing) and relaunch it a plain file
    /// replace, no installer to run. `.deb` installs need root; there's no
    /// clean way to obtain that from a running desktop app without a
    /// polkit prompt, so those are reported as unsupported rather than
    /// guessed at. Best-effort: only the AppImage path has been exercised
    /// (download/verify), not the actual in-place replace + relaunch on a
    /// live app.
    pub fn apply(installer: &Path, app: &tauri::AppHandle) -> EngineResult<()> {
        let target: PathBuf = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok())
            .ok_or_else(|| EngineError::msg("could not locate the running AppImage"))?;

        if installer.extension().and_then(|e| e.to_str()) == Some("deb") {
            return Err(EngineError::msg(
                "this install can't self-update (installed via .deb, which needs sudo) \
                 re-run the install command by hand instead",
            ));
        }

        let script = format!(
            r#"
sleep 1
cp "{installer}" "{target}"
chmod +x "{target}"
exec "{target}"
"#,
            installer = installer.display(),
            target = target.display(),
        );
        Command::new("sh")
            .args(["-c", &script])
            .spawn()
            .map_err(|e| EngineError::io("launch the update script", e))?;
        app.exit(0);
        Ok(())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod platform {
    use std::path::Path;

    use crate::engine::error::{EngineError, EngineResult};

    pub fn apply(_installer: &Path, _app: &tauri::AppHandle) -> EngineResult<()> {
        Err(EngineError::msg("no update path for this OS yet"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_compares_correctly() {
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.1.0", "1.0.0"));
        assert!(!is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0-rc.1"), "prerelease tags don't parse as a plain triple");
        assert!(!is_newer("garbage", "0.1.1"));
    }

    #[test]
    fn finds_checksum_in_binary_mode_sums_file() {
        let sums = "\
20dbd93a6935f52b048c4586ba9728183cccc93d23eccb8b12cae1803f236398 *Fella_0.1.0_amd64.AppImage
4af8884c9c42980d26101e3fc36924b61fa69893a814cc4296c43e18def1748e *Fella_0.1.0_x64-setup.exe
";
        assert_eq!(
            find_checksum(sums, "Fella_0.1.0_x64-setup.exe").as_deref(),
            Some("4af8884c9c42980d26101e3fc36924b61fa69893a814cc4296c43e18def1748e")
        );
        assert_eq!(find_checksum(sums, "Fella_0.1.0_x64_en-US.msi"), None);
    }

    #[test]
    fn finds_checksum_with_two_space_text_mode_form() {
        let sums = "deadbeef  Fella_0.1.0_amd64.deb\n";
        assert_eq!(find_checksum(sums, "Fella_0.1.0_amd64.deb").as_deref(), Some("deadbeef"));
    }

    #[test]
    fn asset_candidates_are_non_empty_on_supported_platforms() {
        // Whichever OS this test runs on, if it's one of the three shipped
        // platforms there must be at least one candidate name to look for.
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        assert!(!asset_candidates("0.1.1").is_empty());
    }
}
