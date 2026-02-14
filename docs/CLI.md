# CLI Reference

Primary binary: `lumen`
Companion binaries: `lpm`, `lpx`

Clap-generated `help` commands are omitted from the lists below.

## `lumen` top-level commands (current)

<!-- BEGIN LUMEN_TOP_LEVEL_COMMANDS -->
- `check`
- `run`
- `emit`
- `trace`
- `cache`
- `init`
- `repl`
- `pkg`
- `fmt`
- `doc`
- `lint`
- `test`
- `ci`
- `build`
<!-- END LUMEN_TOP_LEVEL_COMMANDS -->

## Package commands

<!-- BEGIN LUMEN_PKG_COMMANDS -->
- `init`
- `build`
- `check`
- `add`
- `remove`
- `list`
- `install`
- `update`
- `search`
- `pack`
- `publish`
<!-- END LUMEN_PKG_COMMANDS -->

## `lumen trace` commands (current)

<!-- BEGIN LUMEN_TRACE_COMMANDS -->
- `show`
<!-- END LUMEN_TRACE_COMMANDS -->

## `lumen cache` commands (current)

<!-- BEGIN LUMEN_CACHE_COMMANDS -->
- `clear`
<!-- END LUMEN_CACHE_COMMANDS -->

## `lumen build` commands (current)

<!-- BEGIN LUMEN_BUILD_COMMANDS -->
- `wasm`
<!-- END LUMEN_BUILD_COMMANDS -->

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

Companion-binary note:

- `lpm` mostly mirrors `lumen pkg` and additionally exposes `lpm info [target]`.
- `lpx` executes a source file/package entrypoint: `lpx <file> [--cell <name>] [--trace-dir <dir>]`.
