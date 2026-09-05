# Releasing Fella

Internal maintainer runbook for cutting v0.1 the first public build and the
repo move it ships from. Not user-facing.

## Shape of the v0.1 release

- **Fresh public repo.** v0.1 ships from a new `github.com/Avijit-Kumar-GIT/fella`
  created with a clean `git init` (v0.1.0 is the first commit and the first tag).
  The current `fella-ai` repo **stays private and untouched** as a personal
  history archive `docs/DECISIONS.md` + `docs/ARCHITECTURE.md` carry the "why"
  and "how" forward.
- **Installers** are built by the `release.yml` matrix (macOS universal +
  Windows + Linux x86_64) via `tauri-apps/tauri-action`, on a pushed `v*` tag.
  No local cross-compiling.
- **Unsigned.** A `SHA256SUMS` file (attached by `release.yml`, verified by
  the install scripts and by `/update`) is the integrity check. `/update`
  (post-v0.1.0) checks GitHub for a newer tag and installs it when the user
  runs it; there's still no automatic/background check see "Not in v0.1".
- **`fella-extensions`** ships as a **public stub** the schema, the pack
  rules, `scripts/build-catalog.mjs`, and `catalog.json` with `"packs": []`. No
  real packs. `/packs install` therefore returns a clean "no pack" message; the
  in-app `/packs add <local path>` flow works offline.
- **`fella-web`** (marketing + marketplace sites) stays **private** the hosted
  browse site and the install-counter proxy are paused (`docs/DECISIONS.md`,
  2026-09-02) until there's demand. Nothing about them is part of v0.1.

## State going in (all resolved)

- `LICENSE` present (MIT, © 2026 Avijit Kumar). `Cargo.toml` `authors`,
  `tauri.conf.json` `publisher`/`copyright`, `package.json` `license` all agree.
- Versions aligned at `0.1.0` (`package.json`, `Cargo.toml`, `Cargo.lock`;
  `tauri.conf.json` derives from `package.json`). MSRV `1.88` everywhere.
- `.github/workflows/{ci.yml,release.yml}` exist, actions SHA-pinned.
  `ci.yml`: `pnpm check` + `pnpm build` + `clippy -D warnings` + `cargo test`
  (SQLite default features) on Linux, plus an advisory-only `audit` job
  (`cargo audit` + `pnpm audit`, both currently clean).
- `tauri.conf.json` `app.security.csp` set (conservative; `script-src` keeps
  `'unsafe-inline'` for the SvelteKit bootstrap tightening tracked for v0.1.1).
- Bundle metadata complete; targets are the explicit list
  `["app","dmg","deb","appimage","nsis","msi"]`.
- `.gitignore` covers `auth.json*`, `*.db`, local tooling (`CLAUDE.md`,
  `.claude/`, `.agents/`, `skills-lock.json`), `STUDY.md`, `demo-data/`.

## 1. Pre-flight verification gate

Run on the current tree; nothing tags until this is green. Record numbers in
`docs/PERFORMANCE.md` and the sign-off in `docs/SECURITY-REVIEW-v0.1.md`.

- [x] `cd src-tauri && cargo test --locked` (SQLite default features)
      — 2026-09-04, commit `cfca646`: 108 tests, 0 failed.
- [x] `cargo clippy --all-targets --locked -- -D warnings` — clean, 0 warnings.
- [x] `pnpm run check` 0 / 0 · `pnpm run build` — 199 files, 0 errors/warnings;
      build ~2s.
- [x] `cargo audit` + `pnpm audit --prod` clean (or every advisory triaged)
      — `cargo audit`: 19 unmaintained/unsound/yanked *warnings* on transitive
      GTK/glib deps, no CVE-level vulnerability; `pnpm audit --prod`: no known
      vulnerabilities.
- [x] `scripts/measure.sh --build` binary size, crate counts, frontend bundle
      — recorded 2026-08-30, commit `67c3508` (`docs/PERFORMANCE.md`).
- [ ] `pnpm tauri build` on Linux → `.deb` + `.AppImage`; **record each
      installer's size**. Launch the AppImage: open a folder, ask a question
      (local Ollama or a keyed provider), check the evidence fold, the `/login`
      rejected-key path, `/packs add` a local pack, `/model` switch, mid-run
      stop, tabs + `/focus`. **Still open** the `.deb` built cleanly
      (2026-08-30) but the AppImage has never actually launched here WSLg
      can't create an EGL display in this shell (`EGL_BAD_PARAMETER`), so the
      GUI checklist ran instead against the real **Windows** `rc.1`-`rc.3`
      builds: chat + evidence fold, `run_python`, `run_sql` all exercised (and
      two real bugs found and fixed this way `run_python`'s Windows PATH
      parsing, `run_sql`'s over-broad `replace` ban see PRs #5, #6). Not yet
      exercised on *any* GUI build: `/login` rejected-key path, `/packs add`,
      `/model` switch, mid-run stop, tabs + `/focus`. Needs either a real
      Linux display or accepting Windows-only GUI coverage for v0.1.
- [x] `cargo run --release --example agent_bench` against a local Ollama
      capture the first baseline table into `docs/PERFORMANCE.md`. — captured
      2026-09-04 against `ollama-cloud`/`gemma4:31b` (PR #8); **local** Ollama
      still unmeasured, noted as a gap in `docs/PERFORMANCE.md`.
- [x] Static security review written: the four `SECURITY.md` guarantees
      re-confirmed against the tree, the egress map, CSP active, the
      `run_python`-is-not-a-sandbox caveat that must appear in the release notes.
      — `docs/SECURITY-REVIEW-v0.1.md` exists (from the fresh-repo cut).

## 2. Cut the `fella` repo

- New working dir; `git init`; copy the tree honoring `.gitignore` **verbatim**;
  one commit `chore: initial public release`.
- `gh repo create Avijit-Kumar-GIT/fella --private` (public flip is step 5).
- **URL sweep** `Avijit-Kumar-GIT/fella-ai` → `Avijit-Kumar-GIT/fella`:
  `scripts/install.sh` (`REPO=`), `scripts/install.ps1` (`$repo =`), `README.md`
  (clone / install one-liners / releases links), `CHANGELOG.md` link refs,
  `.github/ISSUE_TEMPLATE/config.yml` discussions URL, `docs/DEV_SETUP.md`, this
  file's self-references. Leave `Avijit-Kumar-GIT/fella-extensions` as-is
  (`src/lib/commands.ts` `MARKETPLACE_URL`, `src-tauri/src/engine/extensions.rs`
  `DEFAULT_CATALOG_URL`).
- Keep `identifier: dev.fella.app` and the `migrate_from_woody` path (harmless;
  slated for removal in v0.2).
- Port `.github/workflows/*` (already SHA-pinned). Turn on branch protection
  requiring the `build` check; enable Discussions.

## 3. Stub `fella-extensions` (public)

- Fresh `git init` from the current private `fella-extensions` tree **minus**
  `packs/nord-theme/` and `packs/personal-finance/`.
- `node scripts/build-catalog.mjs` on the empty `packs/` → `catalog.json` with
  `"packs": []`. Keep the schema files, `CONTRIBUTING.md`, `README.md`,
  `scripts/`, `.github/` + `ci.yml`.
- One commit; create the repo (public, or private then flip alongside `fella`).

## 4. RC and release

- On `fella`: `git tag v0.1.0-rc.1 && git push origin v0.1.0-rc.1` → `release.yml`
  → **draft prerelease** `Fella v0.1.0-rc.1` with the six installers +
  `SHA256SUMS`. (`release.yml` uses the pushed tag verbatim and flags any
  hyphenated tag as a prerelease.)
- Download every artifact; smoke-test on each reachable OS (Linux certain;
  macOS / Windows if available otherwise note the gap and rely on the public
  RC). Run `scripts/install.sh` (Linux, macOS) and `scripts/install.ps1`
  (Windows) end to end against the RC, including the checksum step.
- Fix + `-rc.2` as needed.
- `git tag v0.1.0 && git push origin v0.1.0` → **draft** release `Fella v0.1.0`,
  already flagged as a normal release (not prerelease). Review the assets and
  **un-draft it** the install scripts read `/releases/latest`, which skips
  drafts and prereleases.

## 5. Website + install domain

- Domain: **`lilfella.app`** (marketing). The `fella-web` repo carries the
  static site; `marketing/_redirects` 302s `/install.sh` and `/install.ps1` to
  `raw.githubusercontent.com/Avijit-Kumar-GIT/fella/main/scripts/…`, so the
  README/DEV_SETUP one-liners are `https://lilfella.app/install.{sh,ps1}`.
- That redirect target needs `fella` **public** (step 6) — until then it 404s.
- Cloudflare Pages: connect `fella-web`, output dir `marketing`, custom domains
  `lilfella.app` + `www`. Render `marketing/og.svg` → `og.png` first. Full steps
  in `fella-web/DEPLOY.md`.
- `.app` is HSTS-preloaded HTTPS only; every host CF/Netlify/Vercel does this
  automatically.

## 6. Publish

- Final `git log -p` / secret scan on `fella` and `fella-extensions`.
- `gh repo edit Avijit-Kumar-GIT/fella --visibility public` (and
  `fella-extensions`). This is the point of no return.
- Smoke `curl -fsSL https://lilfella.app/install.sh | sh` once both are live.
- File the "Hosted pack marketplace" issue on `fella` from the `docs/ROADMAP.md`
  bullet (labels `enhancement`, `help wanted`).

## Needs a decision / an owner action

- A screenshot or short GIF for `README.md` (or explicitly defer to v0.1.1).
- macOS / Windows access for RC smoke-testing.
- Whether `fella-extensions` goes public immediately or flips with `fella`.

## Not in v0.1

- Code signing / notarization ship unsigned; `SHA256SUMS` + HTTPS are the
  integrity story.
- The official signed auto-updater (`tauri-plugin-updater`, a signing
  keypair, `latest.json`). `/update` (post-v0.1.0) covers the common case
  the same way without that infrastructure: it reuses the `SHA256SUMS` check
  the install scripts already do, manually triggered, not a background
  updater. See `fella#14` for the tradeoffs.
- Homebrew cask / winget / AUR / Flatpak / Snap.
- A `--features duckdb` bundle (CI-only path).
- The hosted marketplace website, real packs, the install-counter proxy.
- `mcp` as a non-default cargo feature (a noted future lever; today it ships).
- A frontend test harness (`svelte-check` + the manual smoke list for v0.1;
  vitest is a v0.1.1 candidate).
- `script-src` without `'unsafe-inline'` (needs a GUI build to verify the
  SvelteKit hash-mode CSP).
