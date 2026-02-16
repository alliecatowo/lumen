#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <new-version>"
  echo "Example: $0 0.3.0"
  exit 1
fi

NEW_VERSION="$1"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Bumping version to ${NEW_VERSION}..."

# 1. Update workspace Cargo.toml
sed -i "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" "${REPO_ROOT}/Cargo.toml"
echo "  Updated Cargo.toml"

# 2. Update VS Code extension package.json
PACKAGE_JSON="${REPO_ROOT}/editors/vscode/package.json"
if [ -f "$PACKAGE_JSON" ]; then
  # Use node for reliable JSON editing
  node -e "
    const fs = require('fs');
    const pkg = JSON.parse(fs.readFileSync('${PACKAGE_JSON}', 'utf8'));
    pkg.version = '${NEW_VERSION}';
    fs.writeFileSync('${PACKAGE_JSON}', JSON.stringify(pkg, null, 2) + '\n');
  "
  echo "  Updated editors/vscode/package.json"
fi

# 3. Update CHANGELOG.md — add unreleased section if not present
CHANGELOG="${REPO_ROOT}/CHANGELOG.md"
if [ -f "$CHANGELOG" ]; then
  DATE=$(date +%Y-%m-%d)
  if ! grep -q "## \[${NEW_VERSION}\]" "$CHANGELOG"; then
    sed -i "/^# Changelog/a\\
\\
## [${NEW_VERSION}] - ${DATE}\\
\\
### Changed\\
- Version bump to ${NEW_VERSION}" "$CHANGELOG"
    echo "  Updated CHANGELOG.md"
  else
    echo "  CHANGELOG.md already has ${NEW_VERSION} entry"
  fi
fi

# 4. Update tree-sitter-lumen package.json if it exists
TS_PACKAGE="${REPO_ROOT}/tree-sitter-lumen/package.json"
if [ -f "$TS_PACKAGE" ]; then
  node -e "
    const fs = require('fs');
    const pkg = JSON.parse(fs.readFileSync('${TS_PACKAGE}', 'utf8'));
    pkg.version = '${NEW_VERSION}';
    fs.writeFileSync('${TS_PACKAGE}', JSON.stringify(pkg, null, 2) + '\n');
  "
  echo "  Updated tree-sitter-lumen/package.json"
fi

# 5. Update lumen-wasm Cargo.toml if it exists
WASM_TOML="${REPO_ROOT}/rust/lumen-wasm/Cargo.toml"
if [ -f "$WASM_TOML" ]; then
  sed -i "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" "$WASM_TOML"
  echo "  Updated rust/lumen-wasm/Cargo.toml"
fi

echo ""
echo "Version bumped to ${NEW_VERSION}"
echo ""
echo "Next steps:"
echo "  git add -A"
echo "  git commit -m 'release: v${NEW_VERSION}'"
echo "  git push origin main"
echo ""
echo "CI will automatically create the v${NEW_VERSION} tag and trigger all releases."
