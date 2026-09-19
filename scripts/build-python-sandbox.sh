#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SANDBOX_DIR="$REPO_ROOT/python-sandbox"
TARGET_DIR="$SANDBOX_DIR/target/wasm32-unknown-unknown/release"
ARTIFACT="$REPO_ROOT/src-tauri/resources/fella-python-sandbox.wasm"

if ! rustup target list --installed | grep -qx 'wasm32-unknown-unknown'; then
  rustup target add wasm32-unknown-unknown
fi

cd "$SANDBOX_DIR"
cargo build --target wasm32-unknown-unknown --release

mkdir -p "$(dirname "$ARTIFACT")"
cp "$TARGET_DIR/fella_python_sandbox.wasm" "$ARTIFACT"
printf 'wrote %s (%s bytes)\n' "$ARTIFACT" "$(wc -c < "$ARTIFACT")"
