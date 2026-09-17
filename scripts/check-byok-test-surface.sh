#!/usr/bin/env bash
# Keep tests and benchmark entry points independent of a local model server.
# Hosted Ollama Cloud remains allowed because it is an explicit BYOK provider.

set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

# Keep local loopback fake-provider servers allowed; block the well-known
# Ollama server, local setup commands, and legacy env/config spellings.
PATTERN="(?i)localhost:11434|127\\.0\\.0\\.1:11434|fake_ollama|probe_ollama|OLLAMA_|ollama[[:space:]]+(pull|serve)|ollama[[:space:]]*\\(local\\)|local[[:space:]]+ollama|[\"']ollama[\"']"
TARGETS=(src-tauri/tests src-tauri/examples bench other-resources/benchmarks.mdx other-resources/evaluation.mdx)

if matches=$(rg -n -e "$PATTERN" "${TARGETS[@]}" 2>/dev/null); then
	printf '%s\n' "Local Ollama assumptions found in the test/benchmark surface:" >&2
	printf '%s\n' "$matches" >&2
	exit 1
fi

# Unit tests can also be embedded beside the implementation. Keep this scan
# narrower because the runtime legitimately contains the hosted Ollama wire
# adapter and a legacy-id migration alias.
SOURCE_PATTERN="(?i)localhost:11434|127\\.0\\.0\\.1:11434|(env::|env!|option_env!|set_var\\(|var\\(|\\$)[^\\n]{0,80}(FELLA_)?OLLAMA_[A-Z0-9_]+|ollama[[:space:]]+(pull|serve)|ollama[[:space:]]*\\(local\\)|local[[:space:]]+ollama|provider[^\\n]{0,80}[\"']ollama[\"']|[\"']ollama[\"'][^\\n]{0,80}provider"
if matches=$(rg -n -e "$SOURCE_PATTERN" src-tauri/src 2>/dev/null); then
	printf '%s\n' "Local Ollama assumptions found in the Rust source surface:" >&2
	printf '%s\n' "$matches" >&2
	exit 1
fi

echo "BYOK test surface check passed (hosted provider references are allowed)."
