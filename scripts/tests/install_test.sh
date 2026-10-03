#!/bin/sh
# Tests scripts/install.sh against a local fake release served over HTTP.
set -eu

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
INSTALL="$ROOT/scripts/install.sh"
WORK="$(mktemp -d)"
SERVER_PID=""
# shellcheck disable=SC2329
cleanup() { [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true; rm -rf "$WORK"; }
trap cleanup EXIT INT TERM

FAILED=0
ok() { printf 'ok   - %s\n' "$1"; }
bad() { printf 'FAIL - %s\n' "$1"; FAILED=1; }

# Platform asset name the installer will ask for.
case "$(uname -s)" in
  Linux) P=linux ;;
  Darwin) P=macos ;;
  *) echo "skipping: unsupported test platform"; exit 0 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) A=x64 ;;
  arm64|aarch64) A=arm64 ;;
  *) echo "skipping: unsupported test arch"; exit 0 ;;
esac
if [ "$P" = linux ] && [ "$A" = x64 ] && (ldd --version 2>&1 || true) | grep -qi musl; then A=x64-musl; fi
ASSET="lumen-$P-$A.tar.gz"

REL="$WORK/release"; mkdir -p "$REL/pkg"
printf '#!/bin/sh\necho "lumen 9.9.9"\n' > "$REL/pkg/lumen"
printf '#!/bin/sh\necho "lumen-lsp 9.9.9"\n' > "$REL/pkg/lumen-lsp"
chmod +x "$REL/pkg/lumen" "$REL/pkg/lumen-lsp"
(cd "$REL/pkg" && tar czf "../$ASSET" lumen lumen-lsp)
if command -v sha256sum >/dev/null 2>&1; then SUM=$(sha256sum "$REL/$ASSET" | cut -d' ' -f1); else SUM=$(shasum -a 256 "$REL/$ASSET" | cut -d' ' -f1); fi
printf '%s  %s\n' "$SUM" "$ASSET" > "$REL/SHA256SUMS"

PORT=$((20000 + $$ % 20000))
(cd "$REL" && python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1) &
SERVER_PID=$!
i=0; while ! curl -fs "http://127.0.0.1:$PORT/SHA256SUMS" >/dev/null 2>&1; do
  i=$((i + 1)); [ $i -lt 50 ] || { echo "test server did not start"; exit 1; }; sleep 0.1
done
export LUMEN_RELEASE_BASE="http://127.0.0.1:$PORT"

# 1. happy path installs working binaries
D="$WORK/ok"
if sh "$INSTALL" --version v9.9.9 --install-dir "$D" >"$WORK/out1" 2>&1 \
   && [ "$("$D/lumen" --version)" = "lumen 9.9.9" ] && [ -x "$D/lumen-lsp" ]; then ok "installs a verified release"; else bad "installs a verified release"; cat "$WORK/out1"; fi

# 2. a tampered archive is refused and nothing is installed
cp "$REL/$ASSET" "$WORK/asset.bak"
printf 'tampered' >> "$REL/$ASSET"
D="$WORK/tampered"
if sh "$INSTALL" --version v9.9.9 --install-dir "$D" >"$WORK/out2" 2>&1; then bad "rejects a checksum mismatch"; else
  if grep -q "checksum mismatch" "$WORK/out2" && [ ! -e "$D/lumen" ]; then ok "rejects a checksum mismatch"; else bad "rejects a checksum mismatch"; cat "$WORK/out2"; fi
fi
cp "$WORK/asset.bak" "$REL/$ASSET"

# 3. missing SHA256SUMS is refused unless --no-verify is given
mv "$REL/SHA256SUMS" "$WORK/SHA256SUMS.bak"
D="$WORK/nosums"
if sh "$INSTALL" --version v9.9.9 --install-dir "$D" >"$WORK/out3" 2>&1; then bad "refuses without SHA256SUMS"; else
  if grep -q "SHA256SUMS" "$WORK/out3" && [ ! -e "$D/lumen" ]; then ok "refuses without SHA256SUMS"; else bad "refuses without SHA256SUMS"; cat "$WORK/out3"; fi
fi
D="$WORK/noverify"
if sh "$INSTALL" --version v9.9.9 --install-dir "$D" --no-verify >"$WORK/out4" 2>&1 && [ -x "$D/lumen" ]; then ok "--no-verify installs anyway"; else bad "--no-verify installs anyway"; cat "$WORK/out4"; fi
mv "$WORK/SHA256SUMS.bak" "$REL/SHA256SUMS"

# 4. a missing asset (HTTP 404) fails cleanly instead of unpacking an error page
D="$WORK/missing"
if LUMEN_RELEASE_BASE="http://127.0.0.1:$PORT/nope" sh "$INSTALL" --version v9.9.9 --install-dir "$D" >"$WORK/out5" 2>&1; then bad "fails on a 404"; else
  if grep -q "download failed" "$WORK/out5" && ! grep -qi "does not look like a tar" "$WORK/out5"; then ok "fails on a 404"; else bad "fails on a 404"; cat "$WORK/out5"; fi
fi

# 5. bad arguments, and temp dir cleanup
if sh "$INSTALL" --bogus >/dev/null 2>&1; then bad "rejects unknown options"; else ok "rejects unknown options"; fi
mkdir "$WORK/tmpdir"
TMPDIR="$WORK/tmpdir" sh "$INSTALL" --version v9.9.9 --install-dir "$WORK/cleanup" >/dev/null 2>&1 || true
TMPDIR="$WORK/tmpdir" sh "$INSTALL" --version v0.0.0-missing --install-dir "$WORK/cleanup2" >/dev/null 2>&1 || true
if [ -z "$(find "$WORK/tmpdir" -mindepth 1 -print -quit)" ]; then ok "removes its temp directory (success and failure)"; else bad "removes its temp directory"; fi

[ "$FAILED" = 0 ] && echo "all install.sh tests passed"
exit "$FAILED"
