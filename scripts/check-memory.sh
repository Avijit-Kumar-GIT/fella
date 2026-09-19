#!/usr/bin/env bash
# Exercise the disposable Wasm/RustPython store repeatedly in the optimized
# release profile. Override FELLA_MEMORY_ITERATIONS or FELLA_MEMORY_MAX_GROWTH_MB
# when running on a constrained CI runner or a platform with different allocator
# behavior.

set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

cargo run --manifest-path src-tauri/Cargo.toml --release --locked --example memory_probe
