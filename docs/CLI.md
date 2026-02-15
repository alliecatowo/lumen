# Lumen CLI Structure

The Lumen CLI (`lumen`) is the unified interface for the entire Lumen ecosystem. It consolidates previous tools (`lpm`, `lpx`) into a single binary.

## Command Groups

### `lumen pkg`
**Target Audience:** Application Developers
**Purpose:** Manage the package in the current directory.
**Key Commands:**
- `init`: Create a new package.
- `add`/`remove`: Manage dependencies.
- `install`: Install dependencies from `lumen.toml`.
- `build`: Compile the package.
- `publish`: Publish to the registry.

### `lumen wares`
**Target Audience:** Power Users, CI/CD, Registry Ops
**Purpose:** Interact with the Wares Registry and Trust System.
**Key Commands:**
- `login`/`logout`: Authenticate with the registry.
- `whoami`: Check current identity.
- `info`: Inspect remote package metadata without installing.
- `trust-check`: Verify package signatures and provenance.
- `policy`: Manage local trust policies.

> **Note:** `lumen wares` also exposes package management commands (`init`, `build`, etc.) as a convenience, but `lumen pkg` is the recommended interface for project-level workflows.

## Authentication
Authentication is handled via the `lumen wares` command group using OIDC (GitHub).
```bash
lumen wares login
```

## Storage
The registry uses Cloudflare R2 for storage and Cloudflare Workers for the API.
- **Worker**: `workers/registry`
- **Storage**: `wares-registry` bucket
