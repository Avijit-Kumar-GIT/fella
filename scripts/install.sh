#!/bin/sh
# Fella installer for macOS and Linux. Downloads the latest GitHub Release
# build and puts the app in place. An unofficial convenience the same thing
# you'd do by hand from https://github.com/Avijit-Kumar-GIT/fella/releases
#
#   curl -fsSL https://lilfella.app/install.sh | sh
#
# Experimental: builds are unsigned, and this is not yet smoke-tested on every
# OS. Read the script before piping it to a shell.
set -eu

REPO="Avijit-Kumar-GIT/fella"
RELEASES="https://github.com/$REPO/releases/latest"
API="https://api.github.com/repos/$REPO/releases/latest"

die() { echo "install: $*" >&2; echo "Get it by hand: $RELEASES" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

have curl || die "curl is required"

os="$(uname -s)"
arch="$(uname -m)"

echo "Finding the latest Fella release..."
json="$(curl -fsSL --retry 2 -H 'Accept: application/vnd.github+json' "$API" 2>/dev/null)" \
  || die "no published release yet (or GitHub is unreachable)"

# All asset download URLs, one per line (no jq).
urls="$(printf '%s\n' "$json" \
  | grep -o '"browser_download_url": *"[^"]*"' \
  | sed 's/.*"\(https[^"]*\)".*/\1/')"
[ -n "$urls" ] || die "the latest release has no downloadable assets"

pick() { printf '%s\n' "$urls" | grep -i -- "$1" | head -n1; }

tmp="$(mktemp -d "${TMPDIR:-/tmp}/fella.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT INT TERM

# Verify a downloaded file against the release's SHA256SUMS. Builds are
# unsigned, so this is the integrity check. A missing SHA256SUMS is a loud
# warning (older releases predate it); a mismatch is fatal.
sums_url="$(pick '/SHA256SUMS$')"
verify() {
  f="$1"; name="$(basename "$f")"
  if [ -z "$sums_url" ]; then
    echo "install: no SHA256SUMS in this release skipping checksum check" >&2
    return 0
  fi
  if have sha256sum; then sha() { sha256sum "$1" | cut -d' ' -f1; }
  elif have shasum; then sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
  else echo "install: no sha256 tool found skipping checksum check" >&2; return 0
  fi
  curl -fsSL --retry 2 -o "$tmp/SHA256SUMS" "$sums_url" || die "could not fetch SHA256SUMS"
  want="$(grep -E "[ *]${name}\$" "$tmp/SHA256SUMS" | cut -d' ' -f1 | head -n1)"
  [ -n "$want" ] || die "SHA256SUMS has no entry for $name"
  got="$(sha "$f")"
  [ "$want" = "$got" ] || die "checksum mismatch for $name (expected $want, got $got) nothing installed"
  echo "Checksum OK: $name"
}

case "$os" in
Darwin)
  url="$(pick '\.dmg$')"
  [ -n "$url" ] || url="$(pick '\.app\.tar\.gz$')"
  [ -n "$url" ] || die "no macOS build in the latest release"
  file="$tmp/$(basename "$url")"
  echo "Downloading $(basename "$url")..."
  curl -fsSL --retry 2 -o "$file" "$url"
  verify "$file"

  dest="/Applications"
  [ -w "$dest" ] || dest="$HOME/Applications"
  mkdir -p "$dest"

  case "$file" in
  *.dmg)
    mnt="$(mktemp -d "$tmp/mnt.XXXXXX")"
    hdiutil attach -nobrowse -quiet -mountpoint "$mnt" "$file" \
      || die "could not mount the disk image"
    app="$(find "$mnt" -maxdepth 1 -name '*.app' -print -quit)"
    [ -n "$app" ] || { hdiutil detach -quiet "$mnt"; die "no .app inside the disk image"; }
    rm -rf "$dest/$(basename "$app")"
    cp -R "$app" "$dest/"
    hdiutil detach -quiet "$mnt" || true
    ;;
  *.app.tar.gz)
    tar -xzf "$file" -C "$tmp"
    app="$(find "$tmp" -maxdepth 2 -name '*.app' -print -quit)"
    [ -n "$app" ] || die "no .app inside the archive"
    rm -rf "$dest/$(basename "$app")"
    cp -R "$app" "$dest/"
    ;;
  esac

  xattr -dr com.apple.quarantine "$dest/$(basename "$app")" 2>/dev/null || true
  echo "Installed to $dest/$(basename "$app"). Open it from Launchpad or Spotlight."
  ;;

Linux)
  case "$arch" in
  x86_64 | amd64) ;;
  *) die "only x86_64 Linux builds are published right now (you have $arch)" ;;
  esac

  url="$(pick '\.appimage$')"
  if [ -n "$url" ]; then
    file="$tmp/$(basename "$url")"
    echo "Downloading $(basename "$url")..."
    curl -fsSL --retry 2 -o "$file" "$url"
    verify "$file"
    bin="$HOME/.local/bin/fella"
    mkdir -p "$HOME/.local/bin"
    install -m 0755 "$file" "$bin"

    apps="$HOME/.local/share/applications"
    mkdir -p "$apps"
    cat > "$apps/fella.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Fella
Comment=Ask questions about your own files
Exec=$bin
Icon=fella
Categories=Utility;
Terminal=false
EOF
    echo "Installed to $bin (and an app-menu entry)."
    case ":$PATH:" in
      *":$HOME/.local/bin:"*) : ;;
      *) echo "Note: add \$HOME/.local/bin to your PATH to run 'fella' from a terminal." ;;
    esac
  else
    url="$(pick '\.deb$')"
    [ -n "$url" ] || die "no .AppImage or .deb in the latest release"
    have dpkg || die "found only a .deb, but this isn't a Debian/Ubuntu system"
    file="$tmp/$(basename "$url")"
    echo "Downloading $(basename "$url")..."
    curl -fsSL --retry 2 -o "$file" "$url"
    verify "$file"
    echo "Installing the .deb (needs sudo)..."
    if have apt-get; then
      sudo apt-get install -y "$file"
    else
      sudo dpkg -i "$file" || sudo apt-get -f install -y
    fi
    echo "Installed. Launch Fella from your app menu."
  fi
  ;;

*)
  die "unsupported OS: $os (macOS and Linux only; Windows: use install.ps1)"
  ;;
esac

echo
echo "Next: install Ollama (https://ollama.com) and 'ollama pull llama3.1' for a"
echo "local model, or run Fella and type /login to use a hosted one."
