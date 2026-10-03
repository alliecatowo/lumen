#!/bin/sh
# Lumen installer: downloads a release, verifies its SHA-256, installs lumen + lumen-lsp.
#
#   curl -fsSL https://raw.githubusercontent.com/alliecatowo/lumen/main/scripts/install.sh | sh
#   sh install.sh --version v0.6.0
#
# Options (or environment variables):
#   --version <tag>      LUMEN_VERSION      install a specific release tag (default: latest)
#   --install-dir <dir>  LUMEN_INSTALL_DIR  install location (default: /usr/local/bin, else ~/.lumen/bin)
#   --no-verify          LUMEN_NO_VERIFY=1  skip checksum verification (not recommended)
#   LUMEN_RELEASE_BASE   override the download base URL (mirrors, tests)
set -eu

REPO="alliecatowo/lumen"
VERSION="${LUMEN_VERSION:-}"
INSTALL_DIR="${LUMEN_INSTALL_DIR:-}"
NO_VERIFY="${LUMEN_NO_VERIFY:-0}"

if [ -t 1 ]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; BLUE='\033[0;34m'; NC='\033[0m'
else
  RED=''; GREEN=''; BLUE=''; NC=''
fi

info() { printf '%b==>%b %s\n' "$BLUE" "$NC" "$*"; }
fail() { printf '%bError:%b %s\n' "$RED" "$NC" "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --version)
      [ $# -ge 2 ] || fail "--version needs a value"
      VERSION="$2"; shift 2 ;;
    --version=*) VERSION="${1#--version=}"; shift ;;
    --install-dir)
      [ $# -ge 2 ] || fail "--install-dir needs a value"
      INSTALL_DIR="$2"; shift 2 ;;
    --install-dir=*) INSTALL_DIR="${1#--install-dir=}"; shift ;;
    --no-verify) NO_VERIFY=1; shift ;;
    -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) fail "unknown option: $1" ;;
  esac
done

command -v curl >/dev/null 2>&1 || fail "curl is required"

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
FORMAT="tar.gz"
EXE=""

case "$OS" in
  linux) PLATFORM="linux" ;;
  darwin) PLATFORM="macos" ;;
  mingw*|msys*|cygwin*) PLATFORM="windows"; FORMAT="zip"; EXE=".exe" ;;
  *) fail "unsupported OS: $OS" ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH_NAME="x64" ;;
  arm64|aarch64) ARCH_NAME="arm64" ;;
  *) fail "unsupported architecture: $ARCH" ;;
esac

if [ "$PLATFORM" = "linux" ] && [ "$ARCH_NAME" = "x64" ]; then
  # musl-based distributions (Alpine) need the static build.
  if (ldd --version 2>&1 || true) | grep -qi musl; then
    ARCH_NAME="x64-musl"
  fi
fi

if [ "$PLATFORM" = "windows" ] && [ "$ARCH_NAME" != "x64" ]; then
  fail "no Windows build for $ARCH; only x64 is published"
fi

ASSET="lumen-$PLATFORM-$ARCH_NAME.$FORMAT"

# ---------------------------------------------------------------------------
# Resolve the release
# ---------------------------------------------------------------------------
if [ -z "$VERSION" ]; then
  info "Fetching latest release..."
  VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)" || true
  [ -n "$VERSION" ] || fail "could not determine the latest version (GitHub API unreachable or rate limited). Re-run with --version vX.Y.Z"
fi
case "$VERSION" in
  v*) ;;
  *) VERSION="v$VERSION" ;;
esac

BASE="${LUMEN_RELEASE_BASE:-https://github.com/$REPO/releases/download/$VERSION}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

info "Downloading Lumen $VERSION for $PLATFORM-$ARCH_NAME..."
curl -fsSL -o "$TMP_DIR/$ASSET" "$BASE/$ASSET" \
  || fail "download failed: $BASE/$ASSET (no such release asset for this platform?)"

# ---------------------------------------------------------------------------
# Verify integrity
# ---------------------------------------------------------------------------
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  else
    return 1
  fi
}

if [ "$NO_VERIFY" = "1" ]; then
  printf '%bWarning:%b skipping checksum verification\n' "$RED" "$NC" >&2
else
  info "Verifying checksum..."
  curl -fsSL -o "$TMP_DIR/SHA256SUMS" "$BASE/SHA256SUMS" \
    || fail "could not download $BASE/SHA256SUMS; refusing to install an unverified binary (use --no-verify to override)"
  EXPECTED="$(awk -v f="$ASSET" '$2 == f || $2 == "*" f { print $1; exit }' "$TMP_DIR/SHA256SUMS")"
  [ -n "$EXPECTED" ] || fail "$ASSET is not listed in SHA256SUMS"
  ACTUAL="$(sha256_of "$TMP_DIR/$ASSET")" || fail "need sha256sum or shasum to verify the download"
  [ "$EXPECTED" = "$ACTUAL" ] || fail "checksum mismatch for $ASSET (expected $EXPECTED, got $ACTUAL)"
fi

# ---------------------------------------------------------------------------
# Unpack and install
# ---------------------------------------------------------------------------
info "Installing..."
mkdir "$TMP_DIR/out"
if [ "$FORMAT" = "tar.gz" ]; then
  tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR/out" || fail "could not unpack $ASSET"
else
  command -v unzip >/dev/null 2>&1 || fail "unzip is required on Windows"
  unzip -o -q "$TMP_DIR/$ASSET" -d "$TMP_DIR/out" || fail "could not unpack $ASSET"
fi
[ -f "$TMP_DIR/out/lumen$EXE" ] && [ -f "$TMP_DIR/out/lumen-lsp$EXE" ] \
  || fail "archive does not contain lumen$EXE and lumen-lsp$EXE"

if [ -z "$INSTALL_DIR" ]; then
  if [ "$PLATFORM" != "windows" ] && [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
    INSTALL_DIR="/usr/local/bin"
  else
    INSTALL_DIR="$HOME/.lumen/bin"
  fi
fi
mkdir -p "$INSTALL_DIR" || fail "cannot create $INSTALL_DIR"
[ -w "$INSTALL_DIR" ] || fail "$INSTALL_DIR is not writable; re-run with --install-dir <dir> or with sudo"

# Move into place via temp names so an interrupted install never leaves a half-written binary.
for bin in lumen lumen-lsp; do
  cp "$TMP_DIR/out/$bin$EXE" "$INSTALL_DIR/.$bin$EXE.new"
  chmod +x "$INSTALL_DIR/.$bin$EXE.new"
  mv -f "$INSTALL_DIR/.$bin$EXE.new" "$INSTALL_DIR/$bin$EXE"
done

printf '\n%bLumen %s installed to %s%b\n' "$GREEN" "$VERSION" "$INSTALL_DIR" "$NC"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    printf '%bNote:%b %s is not on your PATH. Add this to your shell profile:\n' "$RED" "$NC" "$INSTALL_DIR"
    # shellcheck disable=SC2016
    printf '  export PATH="$PATH:%s"\n' "$INSTALL_DIR"
    ;;
esac
printf "Run '%blumen --version%b' to get started.\n" "$BLUE" "$NC"
