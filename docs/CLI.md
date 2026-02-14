# CLI Reference

Binary: `lumen`

## Top-level commands (current)

`lumen` currently exposes:
`check`, `run`, `emit`, `trace`, `cache`, `init`, `repl`, `pkg`, `fmt`, `doc`, `lint`, `test`, `ci`, `build`.

## Package commands

`lumen pkg` currently exposes:
`init`, `build`, `check`, `add`, `remove`, `list`, `install`, `update`, `search`, `pack`, `publish`.

### `lumen pkg search <query>`

Current behavior:

- Queries a local fixture registry index and prints matching packages.
- Search key matches package `name` and `name@version` (case-insensitive).
- Reads registry path from `LUMEN_REGISTRY_DIR`, defaulting to `.lumen/registry`.

Local fixture-registry note:

- This is filesystem-backed fixture behavior, not remote registry networking.
- Index file is JSON at `<registry-dir>/index.json`.

### `lumen pkg publish [--dry-run]`

Current behavior:

- `lumen pkg publish` (without `--dry-run`) publishes to the local fixture registry:
  writes archive under `<registry-dir>/packages/<name>/<version>/<name>-<version>.tar`
  and updates `<registry-dir>/index.json`.
- `lumen pkg publish --dry-run` validates metadata/content, creates a deterministic archive, and prints content/archive SHA-256 checksums.
- Dry-run archive path is generated under OS temp dir:
  `$(temp-dir)/lumen-publish-dry-run-<pid>-<timestamp>.tar`
  (on Linux fixture runs this is typically under `/tmp/`).

Local fixture-registry note:

- Publish/search use `LUMEN_REGISTRY_DIR` when set; otherwise `.lumen/registry`.
- Non-dry-run publish is local-fixture only (no remote upload/auth flow yet).

Note: `lpm` is the package-manager alias binary for the same flow.
