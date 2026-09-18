#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
data_dir="${AGENT_EVAL_DATA_DIR:-${BENCH_DATA_DIR:-}}"
model="${FELLA_EVAL_MODEL:-ollama-cloud/gemma4:31b}"
iters="${FELLA_EVAL_ITERS:-1}"

if [[ -z "$data_dir" ]]; then
  echo "set AGENT_EVAL_DATA_DIR to a scratch data directory containing fella.db and auth.json" >&2
  exit 2
fi

case "$model" in
  ollama/*|ollama-local/*)
    echo "local Ollama is not supported by this benchmark; use a BYOK hosted provider/model" >&2
    exit 2
    ;;
esac

cd "$repo_root/src-tauri"
cargo run --release --locked --features eval --example agent_eval -- \
  bench \
  --dir "$repo_root/bench/visualization" \
  --models "$model" \
  --iters "$iters" \
  "$@"
