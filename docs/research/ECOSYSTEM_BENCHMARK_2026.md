# Ecosystem & Tooling Benchmark 2026

Date: February 13, 2026
Owner: Research Agent B (Ecosystem & Tooling)

## Executive Summary

Lumen is competitive on core language/runtime direction, but still below ecosystem incumbents on package management, test ergonomics, workspace semantics, and publish workflow maturity.

Hard benchmark result (0-5 per area):

| Area | Incumbent Bar (Best-in-Class) | Lumen Now | Gap |
|---|---:|---:|---:|
| Package manager + lockfile | 5.0 | 1.5 | -3.5 |
| Build + toolchain workflow | 4.5 | 2.0 | -2.5 |
| Test runner | 5.0 | 0.5 | -4.5 |
| Lint | 4.5 | 2.0 | -2.5 |
| Format | 4.5 | 2.0 | -2.5 |
| Doc generation | 4.0 | 1.5 | -2.5 |
| Devtools + workspace UX | 4.5 | 2.0 | -2.5 |
| **Total** | **32.0 / 35** | **11.5 / 35** | **-20.5** |

Primary conclusion: to beat incumbents, Lumen must deliver Cargo-level determinism and one-command project workflows while exploiting its markdown-native advantage (docs + code + tests in one artifact).

## Benchmark Method (Hard Criteria)

Each area is scored 0-5 against these criteria:

1. Determinism and reproducibility (`lock`, `frozen`, integrity, offline).
2. Workspace/monorepo scaling (shared resolution, graph-aware execution).
3. First-party ergonomics (default command quality, discoverability, sane flags).
4. CI and machine output (`--json`, exit code guarantees, stable automation surface).
5. Publish and ecosystem loop (registry, docs, provenance, policy).

## Incumbent Baseline: What Good Looks Like

| Area | Cargo / Rust | npm+pnpm / JS | uv+pip / Python | Go | SwiftPM | Maven/Gradle |
|---|---|---|---|---|---|---|
| Package + lock | `Cargo.lock`, `cargo generate-lockfile`, `cargo publish` | `package-lock.json`, `npm ci`, `npm publish`, `pnpm-lock.yaml` | `uv lock`, `uv sync`, `pip lock` (exp), repeatable installs | `go.mod` + `go.sum` | package manifest + dependency resolution | dependency locking (Gradle), lifecycle-driven dependency mgmt |
| Build | `cargo build/check`, workspace-aware | script runners + workspace execution (`pnpm -r`) | `uv` project orchestration + tools | `go build` in `cmd/go` | `swift build` | `mvn package`, `gradle build` |
| Test | `cargo test` | `npm test`, workspace recursive run | `pytest` + uv tool management | `go test` | `swift test` | Surefire / Gradle test task |
| Lint | `clippy` | ESLint ecosystem | Ruff | `go vet` | SwiftLint ecosystem | Checkstyle/SpotBugs ecosystem |
| Format | `rustfmt` | Prettier ecosystem | Black + Ruff format | `gofmt` | swift-format ecosystem | Spotless ecosystem |
| Docgen | `cargo doc` | Typedoc ecosystem | Sphinx/MkDocs ecosystem | `go doc` | DocC plugin | Javadoc plugin |
| Devtools | mature subcommand + workspace model | strong workspace + store optimizations | environment/tool unification via uv | single `go` tool UX | integrated package workflow | wrapper + lifecycle standardization |

## Where Lumen Stands Now (Repo-Verified)

### Package/Lockfile

Current strengths:

- `lumen pkg` supports local path dependencies and lockfile generation (`rust/lumen-cli/src/pkg.rs:608`, `rust/lumen-cli/src/lockfile.rs:30`).
- Circular dependency detection exists for path graphs (`rust/lumen-cli/src/pkg.rs:721`).

Current blockers:

- No registry install/publish flow; `pkg search` is stubbed (`rust/lumen-cli/src/pkg.rs:662`).
- `pkg update` is explicitly equivalent to install (`rust/lumen-cli/src/pkg.rs:656`).
- Lockfile schema lacks resolver/version metadata and integrity guarantees for path deps (`rust/lumen-cli/src/lockfile.rs:8`).
- Path lock entries may serialize absolute machine-specific paths (observed via `lumen pkg install` in `examples/pkg-demo/app`).

### Build/Toolchain

Current strengths:

- Good base commands: `check`, `run`, `emit`, `pkg build/check`, `build wasm` (`rust/lumen-cli/src/main.rs:47`, `rust/lumen-cli/src/main.rs:167`).
- WASM command checks for `wasm-pack` and gives fallback instructions (`rust/lumen-cli/src/main.rs:544`).

Current blockers:

- No unified top-level project build command (`lumen build` only has wasm subcommand now, `rust/lumen-cli/src/main.rs:124`).
- No explicit incremental build cache model exposed to users (only tool result cache clear).
- No watch mode or CI-centric machine output modes for build/check.

### Test

Current strengths:

- A real test runner implementation exists (`rust/lumen-cli/src/test_cmd.rs:33`).

Current blockers:

- Test runner is not wired into CLI command surface (`rust/lumen-cli/src/main.rs:47` has no `Test` variant).
- Running `lumen test` returns unrecognized subcommand (verified in shell).

### Lint

Current strengths:

- 10 lint rules with warning/error severities (`rust/lumen-cli/src/lint.rs:3`).

Current blockers:

- Parse/tokenize failure returns no diagnostics (silent skip, `rust/lumen-cli/src/lint.rs:77`, `rust/lumen-cli/src/lint.rs:83`).
- No `--format=json`, no baseline/suppression story, no auto-fix path.

### Format

Current strengths:

- AST-based formatter preserving markdown around fenced Lumen blocks (`rust/lumen-cli/src/fmt.rs:1125`).
- `--check` mode integrated (`rust/lumen-cli/src/main.rs:525`).

Current blockers:

- Help text suggests stdin support, but command hard-fails when no files are passed (`rust/lumen-cli/src/main.rs:526`).
- No diff output mode and no range/changed-lines mode for large repos.

### Docgen

Current strengths:

- `lumen doc` supports markdown/json output and AST extraction (`rust/lumen-cli/src/doc.rs:52`).

Current blockers:

- Directory mode is non-recursive (`std::fs::read_dir` one level, `rust/lumen-cli/src/doc.rs:70`).
- No docs-on-publish workflow, no doctest execution, no hosted docs loop.

### Devtools + Workspace UX

Current strengths:

- Trace inspection and cache maintenance are present (`rust/lumen-cli/src/main.rs:78`, `rust/lumen-cli/src/main.rs:83`).
- REPL and provider configuration surface exists (`rust/lumen-cli/src/main.rs:90`, `rust/lumen-cli/src/config.rs`).

Current blockers:

- No first-class Lumen workspace manifest or shared lock semantics.
- Docs are inconsistent with live command surface (for example `docs/CLI.md:1` only documents a subset).

## Concrete CLI/API Design to Surpass Incumbents

### 1) `lumen pkg` Command Surface (Target)

```bash
# Dependency management
lumen pkg add <name>@<range>
lumen pkg add <name> --path ../local-pkg
lumen pkg remove <name>
lumen pkg why <name>
lumen pkg list [--tree]

# Resolution/install
lumen pkg install
lumen pkg install --frozen
lumen pkg install --offline
lumen pkg update [<name>...]
lumen pkg update --latest

# Registry discovery
lumen pkg search <query>
lumen pkg info <name>

# Publish/auth
lumen pkg pack
lumen pkg publish [--dry-run] [--allow-dirty] [--provenance]
lumen pkg login
lumen pkg owner add <name> <user>
```

Design notes:

- `install --frozen` must fail if `lumen.lock` and manifest diverge.
- `install --offline` must never touch network and only use cache/lock.
- `update` must be selective and preserve unrelated lock entries.

### 2) Lockfile Semantics (`lumen.lock` v2)

Required schema upgrades:

1. Top-level lockfile format version (`lockfile_version = 2`).
2. Resolver fingerprint (`resolver = "max-sat-v1"`).
3. Source ID normalization:
   - Registry: `registry+https://.../pkg@1.2.3`.
   - Git: `git+https://...#<commit>`.
   - Path: workspace-relative canonical path (never absolute home-directory paths).
4. Integrity for all immutable artifacts (`sha256-...`).
5. Explicit dependency edges per locked node.
6. Metadata block with generator version and timestamp.

Behavioral rules:

- Deterministic serialization ordering (stable diffs).
- Lock update is atomic (write temp + rename).
- Unknown lockfile versions fail with actionable error.

### 3) Workspace Behavior

Add workspace model in root `lumen.toml`:

```toml
[workspace]
members = ["apps/*", "packages/*"]
exclude = ["experimental/*"]
resolver = "v2"

[workspace.dependencies]
json = "^1.4"
testing = "^0.3"
```

Workspace commands:

```bash
lumen ws graph
lumen ws list
lumen ws run check
lumen ws run test --changed
lumen ws run fmt --check
```

Execution semantics:

- Single root lockfile for workspace.
- Topological execution by dependency graph.
- `--changed` should restrict execution to impacted packages.

### 4) Publish Flow (Registry API + CLI)

Client flow (`lumen pkg publish`):

1. Validate package metadata and semver.
2. Verify clean package contents and deterministic tarball.
3. Run required checks (`fmt --check`, `lint --strict`, `test`, `doc`).
4. Upload with auth token and provenance metadata.
5. Receive immutable version receipt (name, version, checksum, published_at).

Registry API minimum:

- `POST /api/v1/auth/login`
- `GET /api/v1/packages/{name}`
- `GET /api/v1/packages/{name}/{version}/download`
- `POST /api/v1/packages/publish`
- `GET /api/v1/search?q=...`

Publish invariants:

- Version immutability.
- Checksum verification on upload/download.
- Ownership/maintainer ACL.

### 5) Toolchain Commands (Unified UX)

Add first-class top-level workflow:

```bash
lumen build
lumen check
lumen test
lumen lint
lumen fmt
lumen doc
```

Project modes:

- File mode: explicit file path.
- Package mode: nearest `lumen.toml`.
- Workspace mode: root workspace detection.

Machine interfaces:

- `--format=json` for `check`, `test`, `lint`, `doc`.
- Stable exit code contract for CI.

## Exact Steps to Beat Incumbents (Repo-Specific)

### Phase 0 (1 week): Activate Existing Assets

1. Wire `test_cmd` into CLI (`rust/lumen-cli/src/main.rs`, `rust/lumen-cli/src/test_cmd.rs`).
2. Add `lumen test --filter --format=json --jobs` minimum options.
3. Update stale CLI docs to match actual binary behavior (`docs/CLI.md`, `README.md`).

Outcome target: one-command test loop parity with `cargo test`/`go test` baseline ergonomics.

### Phase 1 (2 weeks): Lockfile and Install Determinism

1. Introduce lockfile v2 schema in `rust/lumen-cli/src/lockfile.rs`.
2. Implement `pkg install --frozen` and manifest/lock divergence checks in `rust/lumen-cli/src/pkg.rs`.
3. Ensure path sources serialize workspace-relative paths.
4. Add atomic write + deterministic sort.

Outcome target: CI-reproducible dependency resolution behavior comparable to `npm ci`/`uv sync --locked`.

### Phase 2 (2 weeks): Workspace Semantics

1. Extend config parser for `[workspace]` and `[workspace.dependencies]` (`rust/lumen-cli/src/config.rs`).
2. Add `lumen ws` command group and graph-aware execution.
3. Move lockfile ownership to workspace root.

Outcome target: monorepo UX comparable to Cargo workspaces/pnpm recursive execution.

### Phase 3 (3 weeks): Registry + Publish MVP

1. Implement registry resolution path for `pkg add/install/update/search`.
2. Implement `pkg publish --dry-run` and `pkg pack` deterministic tarball creation.
3. Enforce checksum verification and immutable versions.

Outcome target: end-to-end package ecosystem loop (discover -> install -> publish).

### Phase 4 (2 weeks): Quality Tooling Hardening

1. Lint parse-failure handling must emit diagnostics and fail in strict mode.
2. Add `lint --format=json` + rule ID stability.
3. Add formatter stdin support + `--diff`.
4. Make doc generation recursive and add doctest compile/run mode.

Outcome target: CI and editor integrations on par with mainstream language stacks.

## Success Metrics (Must-Hit)

1. `lumen pkg install --frozen` is deterministic across clean machines.
2. `lumen test` available by default and supports filtering + parallelism.
3. Workspace root lockfile supports 20+ packages without manual ordering.
4. `lumen pkg publish --dry-run` validates full release checklist in <10s for median package.
5. `lumen lint --strict` never silently passes invalid syntax.
6. `lumen doc` recursively covers package/workspace and can run doctest checks.

## Sources

### Cargo / Rust

- Cargo workspaces: https://doc.rust-lang.org/cargo/reference/workspaces.html
- Cargo lockfile and home behavior: https://doc.rust-lang.org/cargo/guide/cargo-home.html#cargolock
- `cargo generate-lockfile`: https://doc.rust-lang.org/cargo/commands/cargo-generate-lockfile.html
- `cargo test`: https://doc.rust-lang.org/cargo/commands/cargo-test.html
- Rust tests run in parallel (book): https://doc.rust-lang.org/book/ch11-02-running-tests.html
- `cargo doc`: https://doc.rust-lang.org/cargo/commands/cargo-doc.html
- `cargo publish`: https://doc.rust-lang.org/cargo/commands/cargo-publish.html
- rustfmt: https://github.com/rust-lang/rustfmt
- clippy: https://github.com/rust-lang/rust-clippy

### npm / pnpm

- `npm ci`: https://docs.npmjs.com/cli/v11/commands/npm-ci
- `package-lock.json`: https://docs.npmjs.com/cli/v11/configuring-npm/package-lock-json
- npm workspaces: https://docs.npmjs.com/cli/v11/using-npm/workspaces
- `npm publish`: https://docs.npmjs.com/cli/v11/commands/npm-publish
- pnpm workspaces: https://pnpm.io/workspaces
- `pnpm install`: https://pnpm.io/cli/install
- `pnpm -r` recursive execution: https://pnpm.io/cli/recursive
- `pnpm publish`: https://pnpm.io/cli/publish

### Python (uv / pip / tooling)

- uv project structure: https://docs.astral.sh/uv/concepts/projects/layout/
- uv lockfile: https://docs.astral.sh/uv/concepts/projects/lockfile/
- uv sync: https://docs.astral.sh/uv/concepts/projects/sync/
- uv tools: https://docs.astral.sh/uv/concepts/tools/
- `pip lock`: https://pip.pypa.io/en/stable/cli/pip-lock/
- pip repeatable installs: https://pip.pypa.io/en/stable/topics/repeatable-installs/
- pytest docs: https://docs.pytest.org/en/stable/
- Ruff docs: https://docs.astral.sh/ruff/
- Black docs: https://black.readthedocs.io/en/stable/

### Go

- Go modules reference: https://go.dev/ref/mod
- Go workspaces tutorial: https://go.dev/doc/tutorial/workspaces
- `cmd/go` command reference (`go build`, `go test`, `go fmt`, `go vet`, `go doc`): https://pkg.go.dev/cmd/go

### SwiftPM

- Swift package manager getting started (`swift package`, `swift build`, `swift test`): https://www.swift.org/get-started/package-manager/
- Swift Package Manager docs hub: https://www.swift.org/documentation/package-manager/
- Swift DocC plugin: https://github.com/swiftlang/swift-docc-plugin

### Maven / Gradle

- Maven build lifecycle: https://maven.apache.org/guides/introduction/introduction-to-the-lifecycle.html
- Maven Surefire plugin (tests): https://maven.apache.org/surefire/maven-surefire-plugin/
- Maven Javadoc plugin: https://maven.apache.org/plugins/maven-javadoc-plugin/
- Gradle dependency locking: https://docs.gradle.org/current/userguide/dependency_locking.html
- Gradle Java testing: https://docs.gradle.org/current/userguide/java_testing.html
- Gradle wrapper: https://docs.gradle.org/current/userguide/gradle_wrapper.html
