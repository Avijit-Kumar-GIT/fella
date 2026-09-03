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

### Hosted (Vercel AI Gateway / OpenAI / xAI)

In the app:

```
/auth                 list the built-in providers and which are signed in
/login vercel         prompt for an API key (masked; not echoed or logged)
/login vercel key <k> paste it on one line instead
/logout vercel        forget that provider's key
```

| id | auth | base URL | embeddings | notes |
|----|------|----------|-----------|-------|
| `ollama` | none local | `http://localhost:11434` | yes | default; the privacy path |
| `openai` | API key | `https://api.openai.com/v1` | yes | broadest audience |
| `vercel` | API key | `https://ai-gateway.vercel.sh/v1` | **yes** | one key → hundreds of models; `creator/model` ids |
| `xai` | API key | `https://api.x.ai/v1` | **no** | Grok |
| `openrouter` | API key | `https://openrouter.ai/api/v1` | **no** | one key → many models; `creator/model` ids (e.g. `google/gemma-2-9b-it:free`); no `default_model`, pick with `/model` |
| `custom` | API key + your own base URL | set via `/model` | depends | any other OpenAI-compatible endpoint |

Keys are stored in `auth.json` (mode `0600`) in Fella's data directory **not**
in the SQLite database, and never in the browser. An API key that was previously
saved in `settings` is migrated into `auth.json` on first launch.

**Vercel AI Gateway:** get a key from the Vercel dashboard → *AI Gateway → API
Keys* (never expires), then `/login vercel` and paste it. The `vercel` row has
**no** `default_model` model ids are provider-namespaced (`openai/gpt-4o-mini`,
`anthropic/…`) and drift so after signing in, run `/model` to see the live list
and pick one. The free tier is rate-limited per model (a `429` that Fella retries
with backoff); buying AI Gateway credits raises the limits.

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
openrouter` and paste it. Like `vercel` it has **no** `default_model` run
`/model` after signing in to pick from the live list. No embeddings endpoint.

**Doc search doesn't need embeddings.** `grep_files`/`read_file` read your
documents directly no index to build, so document search works the same on
every provider, including the ones with no embeddings endpoint (`xAI`,
`OpenRouter`). The `embeddings` column above is still accurate per-provider
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

Verify gates before a PR (see `CONTRIBUTING.md`), all from a clean tree:
`cargo test --locked` and `cargo clippy --all-targets --locked -- -D warnings`
from `src-tauri/` (SQLite default features, which include `mcp`),
`pnpm run check` (0/0), `pnpm run build`. Never `--features duckdb` locally it
is CI-only.

## Packs

Themes, skills, and MCP connectors are **packs**, developed and submitted in the
`fella-extensions` repo, not here. Build one as a directory with a
`fella-pack.json`, test it with `/packs add <path>`, then open a PR there. See
[`EXTENSIBILITY.md`](EXTENSIBILITY.md).

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
