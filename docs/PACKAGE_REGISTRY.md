# Package Registry Design

This document describes the current package-manager behavior and the target design for Lumen's package registry.

## Table of Contents

1. [Overview](#overview)
2. [Current CLI Status (February 2026)](#current-cli-status-february-2026)
3. [Package Format](#package-format)
4. [Version Resolution](#version-resolution)
5. [Registry API Specification](#registry-api-specification)
6. [Storage Strategy](#storage-strategy)
7. [Authentication](#authentication)
8. [Publishing Workflow](#publishing-workflow)
9. [Lock File Format](#lock-file-format)
10. [Local Package Cache](#local-package-cache)
11. [Implementation Roadmap](#implementation-roadmap)

## Overview

Lumen's package management system follows proven patterns from established registries (crates.io, npm, hex.pm) while optimizing for simplicity and the specific needs of AI-native systems.

**Key Design Principles:**
- Start with git-based index for simplicity (upgrade to database later if needed)
- Leverage semantic versioning for compatibility guarantees
- Support both path-based dependencies (local development) and registry dependencies (published packages)
- Lock file guarantees reproducible builds
- Minimal viable registry first, expand features based on community needs

## Current CLI Status (February 2026)

Implemented in `lumen pkg` today:
- Path dependencies (`{ path = "../other-pkg" }`)
- Source discovery for both `.lm` and `.lm.md`
- Dependency graph resolution for local path dependencies (including circular dependency detection)
- `lumen pkg install` / `lumen pkg update` writing `lumen.lock` entries with `path+...` sources
- `lumen pkg search` against a local fixture registry index (`index.json`)
- `lumen pkg pack` deterministic tarball generation (`dist/<name>-<version>.tar`)
- `lpm info [path-or-archive]` metadata + file listing + deterministic checksums
- `lumen pkg publish --dry-run` checklist output with artifact path and SHA-256 checksums
- `lumen pkg publish` local fixture upload path (writes tarball + updates local index)

Not implemented yet:
- Installing registry dependencies from version constraints
- Registry index cloning, semver-based registry resolution, tarball download, checksum verification
- Remote registry upload/authentication flows (networked backend)
- Registry install round-trip (`search` -> `add/install` from registry source)

> The remaining sections describe target registry architecture. Treat them as planned design, not current behavior.

### Phase 2 local fixture-registry behavior (current)

- `lumen pkg search <query>` searches the local fixture index and prints matching package entries.
- `lumen pkg publish` without `--dry-run` writes package archive + metadata into the local fixture registry.
- `lumen pkg publish --dry-run` runs package validation/checksums and writes a temporary archive to:
  `$(temp-dir)/lumen-publish-dry-run-<pid>-<timestamp>.tar`
  (typically `/tmp/...` on Linux fixture runs).
- Fixture routing is controlled by `LUMEN_REGISTRY_DIR` (default `.lumen/registry`).

## Package Format

### lumen.toml Schema

The `lumen.toml` file is the package manifest. It defines package metadata, dependencies, and provider configurations.

```toml
[package]
name = "my-package"              # Required: package name (lowercase, hyphen-separated)
version = "1.0.0"                # Required: semver version string
description = "A cool package"   # Optional: brief description
license = "MIT"                  # Optional: SPDX license identifier
authors = ["Alice <alice@example.com>"] # Optional: list of authors
repository = "https://github.com/user/repo" # Optional: source repository URL
keywords = ["ai", "tools"]       # Optional: search keywords (max 5)
readme = "README.md"             # Optional: path to readme (default: README.md)

[dependencies]
# Registry dependencies (semver ranges; parsed today, install not implemented yet)
http-utils = "^1.2"              # Caret: >=1.2.0, <2.0.0
json-parser = "~0.3"             # Tilde: >=0.3.0, <0.4.0
logging = ">=1.0, <2.0"          # Explicit range
testing = "1.0"                  # Exact match: =1.0.0

# Path dependencies (local development)
mathlib = { path = "../mathlib" }

# Git dependencies (future)
# experimental = { git = "https://github.com/user/repo", tag = "v0.2.0" }

[dev-dependencies]
# Dependencies only needed for tests/examples
test-utils = "^0.1"

[providers]
# Tool provider mappings (unchanged)
"llm.chat" = "openai-compatible"
```

### Package Structure

Standard package layout:

```
my-package/
├── lumen.toml           # Package manifest
├── README.md            # Package documentation
├── src/                 # Source files (.lm or .lm.md)
│   ├── main.lm.md
│   └── lib.lm.md
├── examples/            # Example programs (optional)
│   └── demo.lm.md
└── tests/               # Test files (optional)
    └── test_suite.lm.md
```

### Package Tarball

Published packages are distributed as `.tar.gz` archives containing:
- All `.lm` and `.lm.md` files from `src/`
- `lumen.toml` manifest
- `README.md` (if present)
- `LICENSE` file (if present)

**Excluded from tarball:**
- `lumen.lock` (lock files are project-specific)
- `.lumen/` directory (build artifacts, cache)
- `.git/` directory
- `examples/` and `tests/` (unless explicitly included via config)

## Version Resolution

### Semantic Versioning (SemVer)

Lumen uses [Semantic Versioning 2.0.0](https://semver.org/) for all package versions:

- **MAJOR.MINOR.PATCH** (e.g., `1.4.2`)
- **Breaking changes**: increment MAJOR (e.g., `1.x.x` → `2.0.0`)
- **New features (backward-compatible)**: increment MINOR (e.g., `1.4.x` → `1.5.0`)
- **Bug fixes**: increment PATCH (e.g., `1.4.2` → `1.4.3`)

Pre-release versions are supported: `1.0.0-alpha.1`, `2.0.0-rc.3`

### Version Range Syntax

| Syntax | Meaning | Example |
|--------|---------|---------|
| `^1.2.3` | Caret: compatible updates | `>=1.2.3, <2.0.0` |
| `~1.2.3` | Tilde: patch updates only | `>=1.2.3, <1.3.0` |
| `>=1.0, <2.0` | Explicit range | All versions 1.x.x |
| `1.2.3` | Exact version | Exactly `1.2.3` |
| `*` | Any version | Latest available |

**Default strategy**: Caret ranges (`^`) for maximum compatibility.

### Resolution Algorithm

The dependency resolver uses a **maximum semver** strategy:

1. **Collect all dependency constraints** from the package tree (transitive)
2. **For each package**, find the highest version satisfying all constraints
3. **Detect conflicts**: If no version satisfies all constraints, fail with error
4. **Output resolved versions** to `lumen.lock`

**Example:**

```
Root depends on:
  - A ^1.2
  - B ^2.0

A 1.2.0 depends on:
  - C ~0.3

B 2.1.0 depends on:
  - C >=0.3, <1.0

Resolution:
  A = 1.2.0 (latest matching ^1.2)
  B = 2.1.0 (latest matching ^2.0)
  C = 0.3.9 (highest matching both ~0.3 and >=0.3,<1.0)
```

**Conflict example:**

```
Root depends on:
  - X ^1.0 (which needs Y ^2.0)
  - Z ^3.0 (which needs Y ^1.5)

Error: Cannot resolve Y (^2.0 conflicts with ^1.5)
```

## Registry API Specification

The registry exposes a REST API for package discovery, metadata retrieval, and publishing.

### Base URL

Production: `https://registry.lumen-lang.org/api/v1`
Local dev: `http://localhost:8080/api/v1`

### Endpoints

#### 1. Package Index (Read-Only)

**GET /packages/{name}**

Returns metadata for all versions of a package.

Response (200 OK):
```json
{
  "name": "http-utils",
  "versions": [
    {
      "version": "1.2.0",
      "description": "HTTP utilities for Lumen",
      "license": "MIT",
      "authors": ["Alice <alice@example.com>"],
      "repository": "https://github.com/lumen/http-utils",
      "keywords": ["http", "network"],
      "dependencies": {
        "json-parser": "^0.3"
      },
      "checksum": "sha256:abc123...",
      "published_at": "2026-02-01T12:00:00Z"
    },
    {
      "version": "1.1.0",
      "...": "..."
    }
  ]
}
```

Response (404 Not Found):
```json
{
  "error": "package not found"
}
```

#### 2. Package Tarball Download

**GET /packages/{name}/{version}/download**

Returns the package tarball (`.tar.gz`).

Response: Binary data (`application/gzip`)
Headers: `Content-Disposition: attachment; filename="http-utils-1.2.0.tar.gz"`

#### 3. Package Search

**GET /search?q={query}&limit={N}**

Search packages by name, description, or keywords.

Response (200 OK):
```json
{
  "results": [
    {
      "name": "http-utils",
      "version": "1.2.0",
      "description": "HTTP utilities for Lumen",
      "downloads": 1523
    }
  ],
  "total": 1
}
```

#### 4. Package Publishing

**POST /packages/publish**

Publish a new package version.

Headers:
- `Authorization: Bearer <API_TOKEN>`
- `Content-Type: multipart/form-data`

Body:
- `tarball`: The package `.tar.gz` file
- `metadata`: JSON metadata (optional, extracted from tarball if omitted)

Response (201 Created):
```json
{
  "success": true,
  "name": "http-utils",
  "version": "1.2.0",
  "url": "https://registry.lumen-lang.org/packages/http-utils/1.2.0"
}
```

Response (400 Bad Request):
```json
{
  "error": "invalid version format",
  "details": "version must follow semver (e.g., 1.0.0)"
}
```

Response (403 Forbidden):
```json
{
  "error": "insufficient permissions",
  "details": "you are not an owner of package 'http-utils'"
}
```

#### 5. User Authentication

**POST /auth/token**

Generate an API token for publishing.

Body (JSON):
```json
{
  "username": "alice",
  "password": "secret"
}
```

Response (200 OK):
```json
{
  "token": "lm_abc123...",
  "expires_at": "2026-03-01T12:00:00Z"
}
```

## Storage Strategy

### Phase 1: Git-Based Index (MVP)

Following the proven crates.io model, use a **git repository** as the package index.

**Structure:**

```
registry-index/
├── config.json          # Registry metadata
├── 1/                   # Single-letter packages
│   └── h
├── 2/                   # Two-letter packages
│   └── ht
│       └── http
├── 3/                   # Three-letter packages
│   └── abc/
└── ht/                  # 4+ letter packages
    └── tp/
        └── http-utils   # One file per package (newline-delimited JSON)
```

**Package index file** (`http-utils`):
```
{"name":"http-utils","vers":"1.0.0","deps":[{"name":"json-parser","req":"^0.3"}],"cksum":"sha256:abc...","yanked":false}
{"name":"http-utils","vers":"1.1.0","deps":[],"cksum":"sha256:def...","yanked":false}
{"name":"http-utils","vers":"1.2.0","deps":[],"cksum":"sha256:ghi...","yanked":false}
```

Each line is a JSON object for one version. Clients can clone the index and read files locally.

**Tarball storage**: S3-compatible object storage (e.g., AWS S3, Backblaze B2, local filesystem)

```
/packages/http-utils/1.0.0/http-utils-1.0.0.tar.gz
/packages/http-utils/1.1.0/http-utils-1.1.0.tar.gz
```

**Advantages:**
- Simple to implement (no database required for MVP)
- Easy to mirror/replicate (clone git repo)
- Append-only (never rewrite history)
- CDN-friendly (clients cache git repo)

**Disadvantages:**
- Git repo grows over time (mitigated by shallow clones)
- No advanced search indexing (mitigated by client-side search for MVP)

### Phase 2: Database Backend (Future)

If the index grows large (10k+ packages), migrate to PostgreSQL for:
- Full-text search
- Advanced filtering (license, keywords, downloads)
- Analytics (download counts, trending packages)

## Authentication

### API Tokens

Users generate API tokens for publishing packages. Tokens are stored hashed (bcrypt) in the database.

**Token format**: `lm_<32-character-random-hex>`

Example: `lm_a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6`

**Scopes** (future):
- `publish` — Publish new package versions
- `yank` — Mark versions as yanked (hidden from install)
- `admin` — Manage package owners

### Package Ownership

Each package has a list of **owners** (user accounts) who can publish new versions.

Initial owner: The user who publishes version `0.1.0`

**Adding owners** (future):
```bash
lumen registry add-owner http-utils bob@example.com
```

## Publishing Workflow

> Planned workflow. `lumen registry ...` commands are not implemented in the current CLI.

### 1. Create Package

```bash
lumen pkg init my-package
cd my-package
# Edit src/main.lm or src/main.lm.md, update lumen.toml
```

### 2. Validate Package

```bash
lumen pkg check          # Type-check all files
lumen pkg build          # Compile package
```

### 3. Login to Registry

```bash
lumen registry login
# Prompts for username/password, saves token to ~/.lumen/credentials
```

### 4. Publish Package

```bash
lumen registry publish
```

**What happens:**
1. CLI reads `lumen.toml` to get name/version
2. Checks if version already exists (error if duplicate)
3. Creates tarball from `src/` directory
4. Computes SHA-256 checksum
5. Uploads tarball to registry via `POST /packages/publish`
6. Updates git index with new version entry
7. Prints success message with package URL

### 5. Install Published Package

Users can now add the package:

```toml
[dependencies]
my-package = "^0.1"
```

```bash
lumen pkg update         # Fetch latest registry index
lumen pkg install        # Download and cache my-package
```

## Lock File Format

### lumen.lock

The lock file records **exact resolved versions** for reproducible builds.

Current behavior: `lumen pkg install`/`update` writes path-based lock entries. `registry+...` entries shown below are target format for future registry support.

**Format:** TOML (human-readable, VCS-friendly)

```toml
# This file is automatically generated by lumen pkg.
# Do not edit manually.

[[package]]
name = "http-utils"
version = "1.2.0"
source = "registry+https://registry.lumen-lang.org"
checksum = "sha256:abc123..."
dependencies = [
  "json-parser 0.3.2",
]

[[package]]
name = "json-parser"
version = "0.3.2"
source = "registry+https://registry.lumen-lang.org"
checksum = "sha256:def456..."
dependencies = []

[[package]]
name = "mathlib"
version = "0.1.0"
source = "path+../mathlib"
dependencies = []
```

**Fields:**
- `name` — Package name
- `version` — Exact version (not a range)
- `source` — Where the package came from (`registry+<URL>` or `path+<path>`)
- `checksum` — SHA-256 hash of tarball (for registry packages only)
- `dependencies` — List of direct dependencies (name + version)

**Lock file behavior:**
- Generated by `lumen pkg install` or `lumen pkg update`
- Committed to VCS (ensures team uses same versions)
- `lumen pkg update` currently runs the same path-resolution flow as `install`
- Ignored by published packages (only projects have lock files)

## Local Package Cache

> Planned for registry-backed installs. Current path dependencies are resolved directly from local filesystem paths.

### Cache Directory Structure

Packages are cached in `~/.lumen/packages/` to avoid re-downloading.

```
~/.lumen/
├── credentials          # API token (from `lumen registry login`)
├── index/               # Cloned registry index (git repo)
│   └── registry.lumen-lang.org/
│       ├── config.json
│       └── ht/tp/http-utils
└── packages/            # Cached package tarballs and extracted sources
    ├── http-utils-1.2.0/
    │   ├── lumen.toml
    │   └── src/
    │       └── lib.lm.md
    └── json-parser-0.3.2/
        ├── lumen.toml
        └── src/
            └── main.lm.md
```

**Cache invalidation:**
- Packages are content-addressed by checksum (immutable)
- `lumen cache clear` removes all cached packages
- `lumen pkg update` fetches latest registry index (git pull)

### Download and Extract Flow

1. **Read lumen.lock** to get exact versions
2. **Check cache** (`~/.lumen/packages/{name}-{version}/`)
   - If present and checksum matches, use cached version
   - If absent, download from registry
3. **Download tarball** from `GET /packages/{name}/{version}/download`
4. **Verify checksum** (SHA-256 from lock file)
5. **Extract tarball** to cache directory
6. **Symlink to project** (future: `node_modules`-style structure)

## Implementation Roadmap

### Phase 1 (completed)

1. `lumen.toml` package metadata parsing (`name`, `version`, `description`, `authors`, `license`, `repository`, `keywords`, `readme`)
2. Dependency spec parsing for `path`, version string, and version+registry forms
3. `lumen pkg` subcommands: `init`, `build`, `check`, `add`, `remove`, `list`, `install`, `update`, `search`, `pack`, `publish`
4. Path dependency resolution with circular dependency detection
5. Lockfile v2 read/write compatibility for path-focused flows (`lumen.lock`)
6. Deterministic archive/checksum primitives (`pkg pack`, `pkg publish --dry-run`)

### Phase 2 (in progress): Registry MVP on local fixture infrastructure

Completed pieces:

1. Search/publish command surfaces are present in CLI.
2. Publish packaging/validation pipeline produces deterministic artifacts and checksums usable by upload flow.

Remaining pieces:

1. Registry-backed `pkg search` implementation.
2. Non-dry-run publish upload path.
3. Local fixture-registry endpoint/path env/config wiring.
4. Integration test for publish/search/install round-trip on fixture registry.

### Later Enhancements

1. Yank support
2. Ownership management CLI flows
3. Git dependencies
4. Private/custom registries
5. Workspace support
6. Package metrics

---

## References

**Research sources:**
- [crates.io Architecture](https://github.com/rust-lang/crates.io/blob/main/docs/ARCHITECTURE.md)
- [npm Registry Architecture](https://blog.npmjs.org/post/75707294465/new-npm-registry-architecture.html)
- [Hex.pm Self-Hosting](https://hex.pm/docs/self-hosting)
- [Semantic Versioning](https://semver.org/)
- [npm semver calculator](https://semver.npmjs.com/)

**Key insights:**
- Git-based index is proven and simple (used by crates.io for years)
- Semver ranges are well-understood by developers
- Lock files are essential for reproducible builds
- Start minimal, expand based on user needs

---

## Summary

Current package-manager baseline is path dependencies plus lockfile generation.
Registry semantics in this document (index, semver resolution, cache, publish/auth API) are the target architecture and still pending implementation.
