#!/usr/bin/env bash
#
# measure.sh Fella's size / speed / memory numbers, in one place.
#
#   ./scripts/measure.sh              quick: sizes, deps, bundle, startup, memory
#   ./scripts/measure.sh --bloat      + per-crate binary breakdown (relinks, ~10 min)
#   ./scripts/measure.sh --build      + time an incremental Rust rebuild
#   ./scripts/measure.sh --build-cold + time a full clean rebuild (~20 min!)
#   ./scripts/measure.sh --min        + build the size-minimised profile
#
# Everything printed is also appended, under a dated heading, to
# docs/PERFORMANCE.md see that file for what each number means.
#
# One-time setup (no sudo):  cargo install cargo-bloat hyperfine

set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1
ROOT=$(pwd)
[ -f "$HOME/.fella_env" ] && . "$HOME/.fella_env"

WANT_BUILD=0 WANT_COLD=0 WANT_MIN=0 WANT_BLOAT=0
for a in "$@"; do
	case "$a" in
	--build) WANT_BUILD=1 ;;
	--build-cold) WANT_COLD=1 ;;
	--min) WANT_MIN=1 ;;
	--bloat) WANT_BLOAT=1 ;;
	-h | --help) sed -n '3,14p' "$0" | sed 's/^#\{0,1\} \{0,1\}//'; exit 0 ;;
	*) echo "unknown flag: $a" >&2; exit 2 ;;
	esac
done

BIN=src-tauri/target/release/fella
export DISPLAY="${DISPLAY:-:0}" WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"; pkill -f "target/release/fella" 2>/dev/null' EXIT

have() { command -v "$1" >/dev/null 2>&1; }
sec() { printf '\n### %s\n\n' "$1"; }

main() {
	echo "## $(date '+%Y-%m-%d %H:%M')  ·  commit $(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo none)"

	sec "Toolchain"
	rustc --version
	cargo --version
	{ node --version && pnpm --version; } 2>/dev/null | paste -sd' · ' -
	uname -sr

	sec "Dependencies"
	(
		cd src-tauri || exit
		u=$(cargo tree -e normal --prefix none 2>/dev/null | sed 's/ (\*)//' | sort -u | grep -c .)
		d=$(cargo tree --depth 1 -e normal --prefix none 2>/dev/null | tail -n +2 | grep -c .)
		echo "unique crates in the graph : $u"
		echo "direct dependencies        : $d"
		echo
		echo "duplicate versions (same crate at >1 version wasted size + build time):"
		cargo tree --duplicates -e normal 2>/dev/null | grep -E '^[a-z0-9_-]+ v' | sort -u || echo "  none"
	)

	sec "Release build"
	(
		cd src-tauri || exit
		[ "$WANT_COLD" = 1 ] && cargo clean
		# cargo itself is the up-to-date check; this is a no-op when nothing changed.
		if have hyperfine && [ "$WANT_COLD" = 1 ]; then
			hyperfine --runs 1 --show-output 'cargo build --release'
		else
			/usr/bin/time -f 'wall %es · peak %MKB' cargo build --release
		fi
	)

	if [ "$WANT_BUILD" = 1 ] && [ "$WANT_COLD" != 1 ]; then
		sec "Incremental rebuild time (edit one file, rebuild)"
		(
			cd src-tauri || exit
			if have hyperfine; then
				hyperfine --warmup 0 --runs 3 --prepare 'touch src/lib.rs' 'cargo build --release'
			else
				touch src/lib.rs && /usr/bin/time -f 'wall %es' cargo build --release
			fi
		)
	fi
	echo
	echo "per-crate compile times: cd src-tauri && cargo build --release --timings"
	echo "  then open src-tauri/target/cargo-timings/cargo-timing.html"

	sec "Binary size"
	if [ -x "$BIN" ]; then
		echo "with symbols : $(du -h --apparent-size "$BIN" | cut -f1)  ($(stat -c%s "$BIN") bytes)"
		if strip -s -o "$TMP/w" "$BIN" 2>/dev/null; then
			echo "stripped     : $(du -h --apparent-size "$TMP/w" | cut -f1)  ($(stat -c%s "$TMP/w") bytes)  <- what actually ships"
		fi
		echo
		size "$BIN" 2>/dev/null | sed 's/^/  /'
	else
		echo "(no release binary yet)"
	fi

	if [ "$WANT_MIN" = 1 ]; then
		sec "Binary size release-min profile (opt-level z, fat LTO, abort, stripped)"
		(cd src-tauri && cargo build --profile release-min)
		M=src-tauri/target/release-min/fella
		[ -x "$M" ] && echo "release-min : $(du -h --apparent-size "$M" | cut -f1)  ($(stat -c%s "$M") bytes)"
	fi

	sec "Binary composition (cargo-bloat)"
	if [ "$WANT_BLOAT" != 1 ]; then
		echo "(skipped pass --bloat; it relinks the LTO binary, ~10 min)"
	elif have cargo-bloat; then
		(
			cd src-tauri || exit
			echo "how many bytes of the binary each crate's code occupies:"
			cargo bloat --release --crates -n 18 2>/dev/null
		)
	else
		echo "(cargo-bloat not installed run: cargo install cargo-bloat)"
	fi

	sec "Frontend bundle"
	if have pnpm; then
		pnpm run build 2>&1 | grep -E 'kB │|built in|Wrote site' | tail -20
		echo
		echo "build/ on disk  : $(du -sh build 2>/dev/null | cut -f1)"
		g=$(find build -name '*.js' -exec cat {} + 2>/dev/null | gzip -c | wc -c)
		echo "all JS, gzipped : $((g / 1024)) KB"
	else
		echo "(pnpm not found source ~/.fella_env)"
	fi

	sec "Cold start -> interactive"
	if [ -x "$BIN" ]; then
		echo "(a window opens briefly for each run)"
		for i in 1 2 3; do
			l=$(timeout 15 "./$BIN" 2>&1 | grep -m1 'interactive in' || true)
			[ -n "$l" ] && echo "run $i: ${l#fella: }" || echo "run $i: no timing line (no display / WSLg?)"
			pkill -f 'target/release/fella' 2>/dev/null
			sleep 1
		done
	else
		echo "(no release binary yet)"
	fi

	sec "Idle memory"
	if [ -x "$BIN" ]; then
		"./$BIN" >/dev/null 2>&1 &
		pid=$!
		sleep 6
		if kill -0 "$pid" 2>/dev/null; then
			m=$(ps -o rss= -p "$pid" 2>/dev/null | awk '{print int($1/1024)}')
			h=$(pgrep -cf 'WebKitWebProcess|WebKitNetworkProcess' 2>/dev/null || echo 0)
			echo "main process RSS : ${m:-?} MB   (+ $h WebKit helper process(es), not summed)"
		else
			echo "(process exited early no display?)"
		fi
		kill "$pid" 2>/dev/null
		pkill -f 'target/release/fella' 2>/dev/null
		/usr/bin/time -v timeout 8 "./$BIN" 2>&1 |
			grep -E 'Maximum resident set size|Elapsed \(wall' | sed 's/^[[:space:]]*/  /'
		pkill -f 'target/release/fella' 2>/dev/null
	else
		echo "(no release binary yet)"
	fi

	sec "Notes"
	cat <<'EOF'
- GUI metrics (cold start, memory) need a display WSLg on Windows.
- `time -v` "Maximum resident set size" is the main process only; WebKit
  helpers add ~20-60 MB more.
- `du --apparent-size` = file bytes, not blocks-on-disk.
- First `cargo build --release` is slow (DuckDB C++); later ones are fast.
EOF
}

main 2>&1 | tee "$TMP/report.md"
{ echo; cat "$TMP/report.md"; } >>"$ROOT/docs/PERFORMANCE.md"
echo
echo "→ appended to docs/PERFORMANCE.md"
