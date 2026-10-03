#!/usr/bin/env bash
# Bump the Lumen workspace version.
#
#   scripts/bump-version.sh <new-version> [--with-tree-sitter] [--with-extension]
#
# Always updates: the workspace version, the inter-crate `version = "x"` pins on
# path dependencies, rust/lumen-wasm, CHANGELOG.md, and Cargo.lock.
# The VS Code extension (tagged vscode-v*) and tree-sitter-lumen are versioned
# independently; pass the flags to bump them to the same version.
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "Usage: $0 <new-version> [--with-tree-sitter] [--with-extension]"
  echo "Example: $0 0.7.0"
  exit 1
fi

NEW_VERSION="$1"
shift
WITH_TS=0
WITH_EXT=0
for arg in "$@"; do
  case "$arg" in
    --with-tree-sitter) WITH_TS=1 ;;
    --with-extension) WITH_EXT=1 ;;
    *) echo "unknown option: $arg"; exit 1 ;;
  esac
done

if ! printf '%s' "$NEW_VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
  echo "error: '${NEW_VERSION}' is not a semver version"
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# sed -i differs between GNU and BSD; use a temp file instead.
sub() { # sub <file> <sed-expr>
  sed -E "$2" "$1" > "$1.tmp" && mv "$1.tmp" "$1"
}

echo "Bumping version to ${NEW_VERSION}..."

# 1. Workspace version (only the first top-level `version =`, inside [workspace.package])
sub Cargo.toml "1,/^version = \".*\"/ s/^version = \".*\"/version = \"${NEW_VERSION}\"/"
echo "  Updated Cargo.toml"

# 2. Inter-crate pins: `lumen-x = { path = "../lumen-x", version = "..." }`
for manifest in rust/*/Cargo.toml; do
  if grep -Eq 'path = "\.\./lumen-[a-z-]+", *version = "' "$manifest"; then
    sub "$manifest" "s/(path = \"\\.\\.\\/lumen-[a-z-]+\", *version = \")[^\"]*(\")/\\1${NEW_VERSION}\\2/"
    echo "  Updated path-dependency pins in $manifest"
  fi
done

# 3. Standalone crates that are not workspace members
for manifest in rust/lumen-wasm/Cargo.toml rust/lumen-bench/Cargo.toml; do
  if [ -f "$manifest" ] && ! grep -q 'version.workspace *= *true' "$manifest"; then
    sub "$manifest" "1,/^version = \".*\"/ s/^version = \".*\"/version = \"${NEW_VERSION}\"/"
    echo "  Updated $manifest"
  fi
done

set_json_version() { # set_json_version <package.json>
  node -e "
    const fs = require('fs');
    const pkg = JSON.parse(fs.readFileSync('$1', 'utf8'));
    pkg.version = '${NEW_VERSION}';
    fs.writeFileSync('$1', JSON.stringify(pkg, null, 2) + '\n');
  "
  echo "  Updated $1"
}

# 4. Independently versioned packages (opt-in)
if [ "$WITH_EXT" = 1 ] && [ -f editors/vscode/package.json ]; then set_json_version editors/vscode/package.json; fi
if [ "$WITH_TS" = 1 ] && [ -f tree-sitter-lumen/package.json ]; then set_json_version tree-sitter-lumen/package.json; fi

# 5. CHANGELOG.md — add an entry if not present
CHANGELOG="CHANGELOG.md"
if [ -f "$CHANGELOG" ]; then
  DATE=$(date +%Y-%m-%d)
  if ! grep -q "## \[${NEW_VERSION}\]" "$CHANGELOG"; then
    awk -v v="$NEW_VERSION" -v d="$DATE" '
      { print }
      !done && /^# Changelog/ { print ""; print "## [" v "] - " d; print ""; print "### Changed"; print "- Version bump to " v; done=1 }
    ' "$CHANGELOG" > "$CHANGELOG.tmp" && mv "$CHANGELOG.tmp" "$CHANGELOG"
    echo "  Updated CHANGELOG.md"
  else
    echo "  CHANGELOG.md already has ${NEW_VERSION} entry"
  fi
fi

# 6. Refresh Cargo.lock for the new workspace versions without touching dependencies
if command -v cargo >/dev/null 2>&1; then
  cargo update --workspace --quiet
  echo "  Refreshed Cargo.lock"
  if [ -f rust/lumen-wasm/Cargo.lock ]; then
    (cd rust/lumen-wasm && cargo update --workspace --quiet) && echo "  Refreshed rust/lumen-wasm/Cargo.lock"
  fi
else
  echo "  cargo not found: run 'cargo update --workspace' before committing"
fi

echo ""
echo "Version bumped to ${NEW_VERSION}"
echo ""
echo "Next steps:"
echo "  git add -A"
echo "  git commit -m 'release: v${NEW_VERSION}'"
echo "  git push origin main"
echo ""
echo "After CI is green on main, auto-release.yml tags v${NEW_VERSION} and starts the binary release."
echo "Publishing to crates.io / npm is a separate manual run: gh workflow run publish.yml --ref v${NEW_VERSION}"
echo "The VS Code extension releases from its own vscode-v<version> tag."
