# Security review v0.1

A pre-release self-review of the shipped desktop app. Scope: the base app and
its default (`pdf`, `xlsx`, `mcp`) build. Companion to `SECURITY.md` (the
guarantees) and `docs/AUDIT.md` (the 2026-08-27 philosophy assessment).

Reviewed against commit at branch cut. Re-confirm the checklist items marked
"gate" in the pre-flight (`docs/RELEASE.md` §1) before tagging.

## The four guarantees hold

| Guarantee | Evidence |
|---|---|
| **Read-only workspace** | No `fs` capability is exposed to the webview (`src-tauri/capabilities/default.json` has `core:default` + `dialog:allow-open` + `opener` only). All file access is Rust-side behind the engine. The `run_sql` connection is opened read-only and gated by an allowlist (`engine/data/mod.rs` `ensure_read_only`: single statement, must start SELECT/WITH/…; bans `attach/copy/read_csv/…`). `read_file` resolves a name against the catalogued document list, not a path no traversal. Tests: `workspace.rs`, `fs_tools.rs`, `data/mod.rs`. |
| **Local-first** | One shared `reqwest` client. Every outbound call enumerated below only the model provider is reached on a normal run. No telemetry, update-check, or analytics anywhere in `src-tauri/src`. |
| **Credentials isolated** | `auth.json` written atomically (`auth.json.tmp` + rename) with mode `0600` set on both the temp and final file (`engine/secrets.rs`; test `file_is_owner_only`). Keys are read only to build the `Authorization` header (`engine/llm.rs` `bearer_auth`, `engine/mcp.rs`). `sqlite::save_settings` writes only `provider/base_url/model/embed_model` (test asserts a key in the patch never serializes). Never returned to the frontend (`set_api_key` → `Settings` with a `has_credential: bool`). Never in the conversation archive. |
| **Deterministic, verified answers** | Figures come from tool results; the verification pass re-runs cited queries and checks every number appears in a real result (`engine/verify.rs`; tests in `qc.rs`, `verify.rs`). |

## Network egress the complete list

| Destination | When | Where |
|---|---|---|
| The configured model provider (`localhost:11434` Ollama by default; or OpenAI / Vercel AI Gateway / xAI / Ollama Cloud / OpenRouter / a custom base URL) | every question, plus a health probe and a warm-up | `engine/llm.rs`, `engine/provider.rs` |
| `raw.githubusercontent.com/…/fella-extensions/main/catalog.json` and each pack file it lists | only on `/packs install <id>` | `engine/extensions.rs` (`FELLA_CATALOG_URL` overrides). Every file SHA-256-checked against the catalog before it touches disk. |
| A user-configured MCP server URL | only when an enabled `mcp` connector pack's tool is called | `engine/mcp.rs` |

`run_python` may itself open sockets or read outside the workspace Fella issues
no request on its behalf, and this is a documented limitation, not a regression
(see below).

## Hardening done for v0.1

- **CSP** set (`tauri.conf.json`), replacing `null`: `default-src 'self'`,
  `connect-src 'self' ipc: http://ipc.localhost`, `object-src`/`base-uri`/
  `frame-ancestors`/`form-action` locked. `script-src` retains `'unsafe-inline'`
  for the single SvelteKit hydration bootstrap; the markdown renderer's
  scheme-drop (below) removes the `javascript:`-URI risk that would otherwise
  ride along with it.
- **Markdown output** (`src/lib/markdown.ts`): raw HTML already stripped; now
  link/image renderers drop any non-`http(s)` scheme, so a `[x](javascript:…)`
  originating from prompt-injected file content cannot execute.
- **`opener` capability** narrowed from `https://*` to the six hosts the app
  actually opens.
- **Panic surface**: the two reachable-in-theory `unwrap()/expect()` sites
  (`state.rs` doc-cache lock, `agent.rs` tool-outcome invariant) now degrade to
  `into_inner()` / a returned `EngineError`. Remaining non-test `unwrap()` are
  startup/const-table/`Stdio::piped()` sites that take no user input.
- **Supply chain**: `cargo audit` and `pnpm audit --prod` both clean.
  `calamine` bumped 0.30 → 0.36 to drop the vulnerable `quick-xml 0.37`
  (RUSTSEC-2026-0194/0195, DoS). Remaining `cargo audit` warnings are all
  `unmaintained` gtk-rs GTK3 bindings inherent to Tauri v2 on Linux. Dependabot
  enabled (cargo / npm / actions). CI actions SHA-pinned.
- **Release integrity**: `release.yml` attaches `SHA256SUMS`; `install.sh` /
  `install.ps1` verify the download against it (fatal on mismatch).

## Known limitations for the release notes and first-run copy

1. **Unsigned builds.** No Apple notarization / Windows Authenticode for v0.1.
   Integrity rests on HTTPS + the `SHA256SUMS` file. Users get the OS "unknown
   developer" prompt.
2. **`run_python` is not a sandbox.** It runs `python3 -I` with cleared env, a
   fresh temp cwd, a 20 s wall timeout, and (on Unix) CPU/AS/FSIZE rlimits but
   it can still read files outside the workspace and open the network. It exists
   so the model can compute statistics SQL can't express. Accepted per
   `docs/AUDIT.md`; must be stated plainly where a user would see it.
3. **`mcp` connectors reach the network by design.** Off by default; installing
   and enabling one is the user's explicit choice; the token is theirs. Fella
   vouches only for the review of catalog-listed packs; side-loaded packs are
   marked unverified.
4. **`script-src 'unsafe-inline'`.** Tightening to SvelteKit hash-mode CSP needs
   a GUI build to verify hydration tracked for v0.1.1.
5. **No frontend test harness.** `svelte-check` plus the manual smoke list is
   the v0.1 gate; vitest is a v0.1.1 candidate.

## Not changed (by decision)

The read-only boundary is the safety model there are no write/move/delete
tools, no permission dialogs, no `Results/` artifact area (`docs/AUDIT.md`,
2026-08-27 resolution). `mcp` ships as a default cargo feature.
