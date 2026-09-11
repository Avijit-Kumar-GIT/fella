# Dev setup

> Most people install Fella with the one-line command in the
> [README](../README.md#install) or a
> [release](https://github.com/Avijit-Kumar-GIT/fella/releases) download.
> This page is for **building it yourself**.

Fella needs a Rust toolchain, Node + pnpm, an LLM provider (Ollama by default), and —
on Linux GTK/WebKit system libraries for Tauri.

## 1. System libraries (Linux / Debian-Ubuntu)

```sh
sudo apt-get update && sudo apt-get install -y \
  build-essential pkg-config cmake curl wget file \
  libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev libxdo-dev
```

macOS: install Xcode Command Line Tools (`xcode-select --install`).
Windows: install the Visual Studio C++ Build Tools and WebView2 (ships with Windows 11).

## 2. Rust (stable, **1.88+**)

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
```

MSRV is 1.88 (the default `mcp` feature pulls `rmcp`). The `rusqlite` (and
`duckdb`) crates use the `bundled` feature, so SQLite / DuckDB compile from
source on the first build slow once, fast thereafter. Needs the C/C++ compiler
from step 1.

## 3. Node + pnpm

Any Node 22+ works (pnpm 11 needs it). Example with nvm:

```sh
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
export NVM_DIR="$HOME/.nvm"; . "$NVM_DIR/nvm.sh"
nvm install --lts
npm install -g pnpm
```

## 4. LLM provider

Fella talks to **Ollama on `http://localhost:11434` by default** nothing to
configure, nothing leaves the machine. To use a hosted model instead, sign in
from inside the app with `/login` (see below).

### Ollama (local, default)

```sh
# install from https://ollama.com, then:
ollama pull llama3.1  # chat + tool calling
ollama serve           # if not already running as a service
```

### Hosted providers

In the app:

```
/auth                 list the built-in providers and which are signed in
/login vercel         prompt for an API key (masked; not echoed or logged)
/login vercel key <k> paste it on one line instead
/logout vercel        forget that provider's key
```

| id | auth | base URL | embeddings | notes |
|----|------|----------|-----------|-------|
| id | auth | base URL | embeddings | `default_model` on `/login` |
|----|------|----------|-----------|-------|
| `ollama` | none local | `http://localhost:11434` | yes | `llama3.1` (reconciled to a pulled model) |
| `openai` | API key | `https://api.openai.com/v1` | yes | `gpt-5.6-luna` (cheapest current-gen) |
| `vercel` | API key | `https://ai-gateway.vercel.sh/v1` | **yes** | `openai/gpt-5.6-luna` |
| `xai` | API key | `https://api.x.ai/v1` | **no** | `grok-4.3` (cheapest current grok) |
| `ollama-cloud` | API key | `https://ollama.com` | **no** | `gemma4:31b` (`gemma4:31b-cloud` if that stops resolving) |
| `openrouter` | API key | `https://openrouter.ai/api/v1` | **no** | `openai/gpt-5.6-luna` |
| `custom` | API key + your own base URL | set via `/model` | depends | — (set with `/model`) |

Every default is a cheap, current model; hosted ids drift, so if one 404s after
`/login`, `/model <name>` picks from the live `/models` list. Only text-generation
models appear in that list embeddings, image, audio and moderation ids are
filtered out (you can still name one explicitly with `/model <id>`).

Keys are stored in `auth.json` (mode `0600`) in Fella's data directory **not**
in the SQLite database, and never in the browser. An API key that was previously
saved in `settings` is migrated into `auth.json` on first launch.

**Vercel AI Gateway:** get a key from the Vercel dashboard → *AI Gateway → API
Keys* (never expires), then `/login vercel` and paste it. Defaults to
`openai/gpt-5.6-luna`; ids are provider-namespaced (`creator/model`) and drift,
so `/model` shows the live list to switch. The free tier is rate-limited per
model (a `429` that Fella retries with backoff); buying AI Gateway credits raises
the limits.

**Why API key and not "Sign in with ChatGPT / Claude":**

- Anthropic prohibits third-party apps from using Claude Pro/Max subscription
  OAuth (it is limited to Claude Code and claude.ai). If Anthropic is ever added
  it will be API-key only.
- OpenAI's ChatGPT/Codex OAuth grant is scoped to *coding assistance*. Fella is
  a read-only analytics tool with no code-writing surface, so that grant does not
  apply a plain `OPENAI_API_KEY` is the correct path.
- xAI/Grok does have a device-code OAuth for SuperGrok / X Premium+, but xAI
  enforces a tier allowlist that rejects some active subscriptions, so a key
  fallback is mandatory regardless. OAuth for Grok is deferred; use a key.

**OpenRouter:** get a key from <https://openrouter.ai/keys>, then `/login
openrouter` and paste it. Defaults to `openai/gpt-5.6-luna`; `/model` switches to
anything in the catalogue. No embeddings endpoint.

**Ollama Cloud:** the same wire as local Ollama, just hosted and behind a key.
Get one from <https://ollama.com/settings/keys>, then `/login ollama-cloud`.
Defaults to `gemma4:31b`; `/api/tags` with the key lists your account's cloud
catalogue (browse `ollama.com/search?c=cloud`) to switch with `/model`.

**Doc search doesn't need embeddings.** `grep_files`/`read_file` read your
documents directly no index to build, so document search works the same on
every provider, including the ones with no embeddings endpoint (`xAI`,
`Ollama Cloud`, `OpenRouter`). The `embeddings` column above is still accurate per-provider
capability info, just currently unused by any feature (see `docs/DECISIONS.md`,
2026-08-29).

### Adding another provider (contributors)

Each provider is **one row** in `PROVIDERS` in
[`src-tauri/src/engine/provider.rs`](../src-tauri/src/engine/provider.rs): `id`,
`display`, `auth` (`None` / `ApiKey`), `base_url`, `default_model`,
`default_embed_model`, `wire` (`Ollama` / `OpenAi`), `embeddings`, `get_key_url`.
Add the row and it shows up in `/auth`, `/login`, and the registry-driven
defaults no other code changes for an OpenAI-compatible endpoint.

## 5. Run

```sh
pnpm install
pnpm tauri dev
```

**WSLg:** if the window opens but the layout looks broken CSS variables/fonts
loading, but no centering, no spacing, flex layout not applying while
individual component styles (buttons) partly do launch with the DMA-BUF
renderer disabled:

```sh
WEBKIT_DISABLE_DMABUF_RENDERER=1 pnpm tauri dev
```

WSLg's virtualised GPU doesn't get along with WebKitGTK's default compositing
path; this forces a fallback that renders correctly. Confirmed working
2026-09-11. If that alone doesn't fully fix it, also try
`WEBKIT_DISABLE_COMPOSITING_MODE=1` alongside it. A harder failure a window
that never opens at all, `EGL_BAD_PARAMETER` is a separate, still-open
problem (`docs/RELEASE.md`, "Not yet exercised") this env var is for a
window that opens but paints wrong, not for that.

Verify gates before a PR (see `CONTRIBUTING.md`), all from a clean tree:
`cargo test --locked` and `cargo clippy --all-targets --locked -- -D warnings`
from `src-tauri/` (SQLite default features, which include `mcp`),
`pnpm run check` (0/0), `pnpm run build`. Never `--features duckdb` locally it
is CI-only.

## Packs

Themes, skills, MCP connectors, and augments are **packs**, developed and
submitted in the `fella-extensions` repo, not here. Build one as a directory
with a `fella-pack.json`. See [`EXTENSIBILITY.md`](EXTENSIBILITY.md) for the
per-kind rules and `fella-extensions/docs/WRITING-A-PACK.md` for the walkthrough.

Two ways to test a pack against a dev build, **neither needs a GitHub push**:

- **`/packs add <path>`** a local pack directory (even an uncommitted one in a
  sibling `fella-extensions` checkout). No network. Fastest loop for iterating
  on a pack's content; it installs **unverified**, same as any side-loaded
  pack. This is enough for a `skill`/`theme`/`augment` pack's actual behaviour
  the manifest, payload, and (for an augment) the command/file wiring are all
  exercised exactly as they would be from the catalog.
- **A local catalog, to exercise `/packs install <id>`** the by-id path real
  users hit, including the SHA-256 check against `catalog.json`. Mirrors how
  `src-tauri/tests/packs_marketplace.rs` tests it, but manually against a real
  running app:
  ```sh
  cd fella-extensions
  node scripts/build-catalog.mjs --base http://127.0.0.1:8787
  python3 -m http.server 8787          # serves catalog.json + packs/ as-is

  # in another shell, same machine:
  cd fella-oss
  FELLA_CATALOG_URL=http://127.0.0.1:8787/catalog.json pnpm tauri dev
  ```
  Then in the app: `/packs install notes` (or any id in your local
  `catalog.json`). `FELLA_CATALOG_URL` overrides the default
  `raw.githubusercontent.com/…/fella-extensions/main/catalog.json`
  (`engine/extensions.rs`); everything downstream install, hash-check,
  write to `<app-data>/extensions/<id>/` runs unmodified. Re-run
  `build-catalog.mjs` after any edit to a pack under `packs/` before
  reinstalling. Same seam `/update` uses for testing an installer apply
  without cutting a real release: an env var pointing at a local
  `python -m http.server`, see `FELLA_RELEASE_API_URL` in `engine/update.rs`.

## Non-interactive shells

If your shell tooling runs commands non-interactively (so `~/.bashrc` is skipped),
source this helper first:

```sh
cat > ~/.fella_env <<'EOF'
. "$HOME/.cargo/env" 2>/dev/null || export PATH="$HOME/.cargo/bin:$PATH"
export NVM_DIR="$HOME/.nvm"
if [ -s "$NVM_DIR/nvm.sh" ]; then
  . "$NVM_DIR/nvm.sh" --no-use
  _b=$(ls -d "$NVM_DIR"/versions/node/*/bin 2>/dev/null | tail -1)
  [ -n "$_b" ] && export PATH="$_b:$PATH"
fi
EOF
```

then `source ~/.fella_env` at the top of each shell.

## Measuring size / speed / memory

See [`PERFORMANCE.md`](PERFORMANCE.md). Short version:

```sh
cargo install cargo-bloat hyperfine   # one-time, no sudo
./scripts/measure.sh                   # numbers + a dated entry in PERFORMANCE.md
```

## Evaluating the agent loop

`examples/agent_bench` times the loop; `examples/agent_eval` **scores** it
(correctness, answer-closeness, wasted tool calls, tokens/correct-answer,
across prompt ablations / folder sizes / models). Both need a data dir with a
**copy** of your real `fella.db` + `auth.json` so they use your provider and
never touch live state.

```sh
cd src-tauri
BENCH_DATA_DIR=/tmp/eval-data cargo run --release --example agent_bench
AGENT_EVAL_DATA_DIR=/tmp/eval-data \
  cargo run --release --features eval --example agent_eval -- accuracy --models "gemma4:31b"
```

Neither runs in CI (they need a live model). See
[`PERFORMANCE.md`](PERFORMANCE.md) for the subcommands and baselines.
