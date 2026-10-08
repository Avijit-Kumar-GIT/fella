#!/usr/bin/env bash
#
# measure.sh Fella's size / speed / memory numbers, in one place.
#
#   ./scripts/measure.sh              engine binary size, deps, bundle, build time
#   ./scripts/measure.sh --bloat      + per-crate binary breakdown (relinks, ~10 min)
#   ./scripts/measure.sh --build      + time an incremental Rust rebuild
#   ./scripts/measure.sh --build-cold + time a full clean rebuild (~20 min!)
#   ./scripts/measure.sh --min        + build the size-minimised profile
#
# To keep a local copy, pipe the output to a file; see docs/PERFORMANCE.md for
# what each number means and how to compare measurements fairly.
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

BIN=backend/target/release/fella
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

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
		cd backend || exit
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
		cd backend || exit
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
			cd backend || exit
			if have hyperfine; then
				hyperfine --warmup 0 --runs 3 --prepare 'touch src/lib.rs' 'cargo build --release'
			else
				touch src/lib.rs && /usr/bin/time -f 'wall %es' cargo build --release
			fi
		)
	fi
	echo
	echo "per-crate compile times: cd backend && cargo build --release --timings"
	echo "  then open backend/target/cargo-timings/cargo-timing.html"

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
		(cd backend && cargo build --profile release-min)
		M=backend/target/release-min/fella
		[ -x "$M" ] && echo "release-min : $(du -h --apparent-size "$M" | cut -f1)  ($(stat -c%s "$M") bytes)"
	fi

	sec "Binary composition (cargo-bloat)"
	if [ "$WANT_BLOAT" != 1 ]; then
		echo "(skipped pass --bloat; it relinks the LTO binary, ~10 min)"
	elif have cargo-bloat; then
		(
			cd backend || exit
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

	sec "Whole-app startup and memory"
	echo "Not measured here: this script profiles the Rust sidecar, not the Electron desktop process tree."
	echo "On Windows, build the sidecar with 'pnpm electron:build', then run:"
	echo "  .\\scripts\\measure-windows.ps1 -Seconds 15"

	sec "Notes"
	cat <<'EOF'
- This script reports Rust sidecar build and binary metrics; it does not launch the Electron UI.
- Use the Windows helper or a platform process-tree profiler for full-app memory.
- `du --apparent-size` = file bytes, not blocks-on-disk.
- First `cargo build --release` is slow; later ones are fast.
EOF
}

main
