# Security Guide

Lumen's package manager includes comprehensive supply-chain security infrastructure to protect against tampering, unauthorized packages, and dependency vulnerabilities. This guide covers the security features built into the Lumen CLI and registry.

## Overview

Supply-chain attacks targeting package registries have become increasingly common. For AI-native tools that execute untrusted code and interact with sensitive systems, security is critical. Lumen addresses this through multiple layers:

- **Package Signing** — Cryptographic signatures using Ed25519
- **Registry Authentication** — OIDC-based identity verification
- **TUF Verification** — The Update Framework for secure metadata
- **Transparency Log** — Merkle tree for tamper-evident publishing
- **Audit Logging** — Structured security event trail
- **Trust Policies** — Configurable rules for dependency verification

## Package Signing

All published packages are cryptographically signed using Ed25519 digital signatures. Publishers generate a signing keypair, and the registry verifies signatures before accepting uploads.

### Key Generation

Generate a new signing keypair:

```bash
lumen wares keygen
```

This creates two files in `~/.lumen/keys/`:
- `signing-key-{key_id}.pub` — 32-byte Ed25519 public key (hex-encoded)
- `signing-key-{key_id}.sec` — 64-byte Ed25519 secret key (hex-encoded)

The key ID is derived from the SHA-256 fingerprint of the public key (first 16 bytes).

Secret keys are stored with `0o600` permissions (owner read/write only).

### Publishing Workflow

When you publish a package with `lumen pkg publish`, the CLI automatically:

1. Computes the SHA-256 content hash of the package tarball
2. Constructs a canonical signing string:
   ```
   lumen:upload:{package}:{version}:{content_hash}:{timestamp}
   ```
3. Signs the canonical string with your secret key
4. Uploads the package with signature metadata

The registry verifies the signature before accepting the upload.

### Signature Format

Signatures include:

| Field | Description |
|-------|-------------|
| `key_id` | Hex-encoded key fingerprint (32 chars) |
| `signature` | Hex-encoded Ed25519 signature (128 chars) |
| `timestamp` | Unix timestamp (seconds since epoch) |
| `algorithm` | Always `"ed25519"` |

### Listing Your Keys

View your registered signing keys:

```bash
lumen wares keys
```

Output shows key IDs, creation dates, and usage counts.

## Registry Authentication

Lumen uses OpenID Connect (OIDC) for registry authentication. This provides secure, token-based access without storing passwords.

### Login Flow

Authenticate with the registry:

```bash
lumen wares login
```

This initiates an **Authorization Code Flow with PKCE** (Proof Key for Code Exchange):

1. CLI generates a random code verifier and S256 code challenge
2. Opens your browser to the OIDC provider's authorization endpoint
3. You authenticate with your identity provider (e.g., GitHub)
4. Provider redirects to `http://localhost:8765/callback` with authorization code
5. CLI exchanges code + verifier for ID token and refresh token
6. Validates ID token claims (issuer, audience, expiration, nonce)
7. Stores tokens securely

### Token Storage

Authentication tokens are stored in one of two locations:

1. **OS Keyring** (preferred) — Uses system credential manager
   - macOS: Keychain
   - Linux: Secret Service API (GNOME Keyring, KWallet)
   - Windows: Credential Manager

2. **File-based** (fallback) — `~/.lumen/credentials.toml` with `0o600` permissions

Tokens use the `lm_` prefix for identification.

### Token Validation

The CLI validates ID tokens by checking:

- **Issuer** — Matches expected provider
- **Audience** — Contains registry client ID
- **Expiration** — Token is not expired
- **Nonce** — Matches PKCE flow nonce

### Refresh Tokens

Access tokens expire after a short duration (typically 1 hour). Refresh tokens allow obtaining new access tokens without re-authentication:

```bash
# Tokens are refreshed automatically when needed
lumen pkg publish
```

### Check Authentication Status

View your current authentication status:

```bash
lumen wares whoami
```

Output shows:
- Logged-in username/email
- Token expiration time
- Registry URL

### Logout

Revoke stored tokens:

```bash
lumen wares logout
```

This removes tokens from the keyring or credentials file.

## TUF Verification

Lumen implements **The Update Framework (TUF)**, a specification for securing software update systems. TUF protects against various attacks including arbitrary package replacement, rollback attacks, and compromised keys.

### TUF Roles

TUF uses four metadata roles with different trust levels:

| Role | Purpose | Keys | Expiration |
|------|---------|------|------------|
| **Root** | Trust anchor; delegates to other roles | Offline, highly secure | 1 year |
| **Targets** | Maps package names to content hashes | Online, protected | 3 months |
| **Snapshot** | Records versions of all metadata | Online, automated | 1 day |
| **Timestamp** | Indicates current snapshot version | Online, automated | 1 day |

### Threshold Signing

Each role supports **threshold signing**, requiring M-of-N signatures:

```toml
[tuf.root]
threshold = 3
keys = ["key-1", "key-2", "key-3", "key-4", "key-5"]
```

This configuration requires 3 valid signatures from the 5 authorized keys. Threshold signing protects against single key compromise.

### Verification Process

When you install a package, the CLI performs TUF verification:

1. **Fetch Timestamp** — Download `timestamp.json` (current snapshot version)
2. **Fetch Snapshot** — Download `snapshot.json` (metadata versions)
3. **Fetch Targets** — Download `targets.json` (package hashes)
4. **Verify Chain** — Validate all signatures against Root metadata
5. **Check Versions** — Ensure monotonically increasing version numbers
6. **Check Expiration** — Reject expired metadata
7. **Verify Package** — Check content hash matches Targets metadata

### Rollback Protection

TUF prevents rollback attacks by requiring monotonically increasing version numbers:

```json
{
  "role": "targets",
  "version": 42,
  "expires": "2025-03-01T00:00:00Z",
  "targets": { ... }
}
```

If the CLI has seen version 42, it will reject version 41 or earlier.

### Root Rotation

The Root role can rotate (replace) its signing keys. To update the trusted Root:

1. New Root metadata is signed by both old and new keys
2. CLI validates new Root against current trusted Root
3. If valid, new Root becomes trusted
4. Subsequent metadata is verified against new Root keys

This allows recovering from key compromise without manual intervention.

### Canonical Format

TUF metadata uses a canonical binary format for signing:

```
{role}:{version}:{expires}:{body}
```

Where:
- `role` — Role name (root, targets, snapshot, timestamp)
- `version` — Integer version number
- `expires` — ISO 8601 expiration timestamp
- `body` — JSON-encoded role-specific data

### Checking TUF Status

View current TUF metadata versions:

```bash
lumen wares tuf-status
```

Output shows:
- Root version and expiration
- Targets, Snapshot, Timestamp versions
- Last update timestamps
- Key IDs for each role

## Transparency Log

Lumen maintains a **Merkle tree transparency log** for all published packages. This provides a tamper-evident, append-only record that anyone can audit.

### How It Works

Each package publication creates a log entry:

```json
{
  "sequence": 1234,
  "timestamp": "2025-02-18T12:34:56Z",
  "package_name": "@acme/utils",
  "package_version": "1.2.3",
  "content_hash": "sha256:abcdef123456...",
  "publisher": "alice@example.com",
  "signature": "ed25519:789abc..."
}
```

Log entries are hashed and organized into a **binary Merkle tree**:

- **Leaf nodes** — Hash of log entry with `0x00` prefix (domain separation)
- **Internal nodes** — Hash of left and right child with `0x01` prefix

The **Merkle root** commits to all entries in the log. Any modification to a past entry changes the root, making tampering detectable.

### Inclusion Proofs

To prove a package is in the log:

```bash
lumen wares verify-inclusion @acme/utils 1.2.3
```

The registry returns:
- Log entry at specific sequence number
- **Merkle proof** — Sibling hashes from leaf to root
- Current Merkle root

The CLI recomputes the root using the proof path and verifies it matches the published root.

### Consistency Proofs

To verify the log has not been tampered with:

```bash
lumen wares verify-consistency
```

The registry returns:
- Previous Merkle root (from CLI's last check)
- Current Merkle root
- **Consistency proof** — Hashes proving the previous tree is a prefix of the current tree

This ensures new entries were appended without modifying history.

### Log Format

The transparency log is serialized as:

```
Header:
  version: u32 (4 bytes)
  size: u64 (8 bytes, entry count)
  root: [u8; 32] (32 bytes, Merkle root)

Entries (tab-separated):
  sequence\ttimestamp\tpackage\tversion\thash\tpublisher\tsignature\n
```

### Auditing the Log

Download the full log for independent auditing:

```bash
lumen wares download-log --output transparency.log
```

Verify the log locally:

```bash
lumen wares audit-log transparency.log
```

This recomputes all hashes and verifies the Merkle tree structure.

## Audit Logging

Lumen records security-relevant operations in a structured audit log. This provides an immutable trail for compliance and incident response.

### Audited Events

The audit log records:

- **Authentication** — Login, logout, token refresh
- **Package Operations** — Publish, yank, unyank
- **Key Management** — Keygen, key registration, key revocation
- **Trust Policy Changes** — Policy updates, trust decisions
- **TUF Operations** — Root rotation, metadata updates

### Log Format

Audit entries are JSON lines:

```json
{
  "timestamp": "2025-02-18T12:34:56.789Z",
  "event_type": "package_publish",
  "actor": "alice@example.com",
  "resource": "@acme/utils@1.2.3",
  "outcome": "success",
  "details": {
    "key_id": "abc123",
    "content_hash": "sha256:def456"
  }
}
```

### Viewing Audit Logs

Display recent audit events:

```bash
lumen wares audit-log
```

Filter by event type:

```bash
lumen wares audit-log --type package_publish
```

Export audit log:

```bash
lumen wares audit-log --export audit-2025-02.json
```

### Vulnerability Scanning

The audit module also scans dependencies for known vulnerabilities:

```bash
lumen wares audit
```

This:
1. Parses `Cargo.lock` (or equivalent) to extract dependencies
2. Queries advisory database for vulnerabilities
3. Checks version constraints against vulnerable versions
4. Reports severity levels (Critical, High, Medium, Low, None)

Vulnerability severity follows CVSS scoring:

| Severity | CVSS Score |
|----------|------------|
| Critical | 9.0 - 10.0 |
| High | 7.0 - 8.9 |
| Medium | 4.0 - 6.9 |
| Low | 0.1 - 3.9 |
| None | 0.0 |

### Detecting Tampering

The audit command also checks for:

- **Missing Checksums** — Packages without content hashes (potential tampering)
- **Checksum Mismatches** — Installed package doesn't match registry hash
- **Unsigned Packages** — Packages lacking valid signatures

## Trust Policies

Lumen's trust policy system lets you define rules for which packages are allowed in your projects.

### Policy Configuration

Define trust policies in `lumen.toml`:

```toml
[trust_policy]
# Allow all packages from trusted namespaces
allow_namespaces = ["@lumen", "@acme"]

# Deny specific packages
deny_packages = ["@evil/backdoor"]

# Require minimum signature threshold
min_signatures = 1

# Require TUF verification
require_tuf = true

# Maximum package age (days)
max_package_age = 365

# Allowed signing key IDs
allowed_keys = ["abc123", "def456"]
```

### Trust Check

Verify a package against trust policies:

```bash
lumen wares trust-check @acme/utils
```

Output shows:
- Policy evaluation result (Allow/Deny)
- Matching rules
- Signature verification status
- TUF verification status
- Package age
- Security advisories

### Policy Management

List active policies:

```bash
lumen wares policy list
```

Add a new policy rule:

```bash
lumen wares policy add --allow-namespace @myorg
```

Remove a policy rule:

```bash
lumen wares policy remove --deny-package @evil/backdoor
```

### CI/CD Integration

Use trust checks in CI pipelines to prevent malicious dependencies:

```yaml
# .github/workflows/security.yml
- name: Check dependencies
  run: lumen wares audit

- name: Verify trust policies
  run: lumen wares trust-check --all
```

## CLI Commands Reference

### Authentication

```bash
# Log in to registry
lumen wares login

# Log out
lumen wares logout

# Check authentication status
lumen wares whoami
```

### Key Management

```bash
# Generate new signing keypair
lumen wares keygen

# List your signing keys
lumen wares keys

# Revoke a key
lumen wares revoke-key <key_id>
```

### Package Operations

```bash
# Publish package (with automatic signing)
lumen pkg publish

# Verify package signature
lumen wares verify @namespace/package@version
```

### TUF Operations

```bash
# Check TUF metadata status
lumen wares tuf-status

# Update TUF metadata
lumen wares tuf-update

# Verify TUF chain
lumen wares tuf-verify
```

### Transparency Log

```bash
# Verify package inclusion in log
lumen wares verify-inclusion @namespace/package@version

# Verify log consistency
lumen wares verify-consistency

# Download full log
lumen wares download-log --output log.txt

# Audit log locally
lumen wares audit-log log.txt
```

### Trust Policies

```bash
# Check package against policies
lumen wares trust-check @namespace/package

# Check all dependencies
lumen wares trust-check --all

# List active policies
lumen wares policy list

# Add policy rule
lumen wares policy add --allow-namespace @org

# Remove policy rule
lumen wares policy remove --deny-package @bad/pkg
```

### Security Auditing

```bash
# Scan dependencies for vulnerabilities
lumen wares audit

# View audit log
lumen wares audit-log

# Export audit log
lumen wares audit-log --export audit.json

# Filter audit log by event type
lumen wares audit-log --type package_publish
```

## Configuration

Security settings in `lumen.toml`:

```toml
[registry]
url = "https://wares.lumen-lang.org"
verify_tls = true

[auth]
# Token storage: "keyring" or "file"
token_storage = "keyring"

[tuf]
# Minimum metadata age before update (seconds)
min_refresh_interval = 3600

# Warn on metadata expiring within (days)
expiration_warning_days = 7

[transparency]
# Verify inclusion proofs for all installs
verify_inclusion = true

# Verify consistency proofs on update
verify_consistency = true

[trust_policy]
allow_namespaces = ["@lumen"]
require_tuf = true
min_signatures = 1
```

## Best Practices

### For Publishers

1. **Protect Signing Keys**
   - Store secret keys offline or in hardware security modules (HSMs)
   - Use separate keys for different projects
   - Rotate keys periodically

2. **Use Strong Authentication**
   - Enable two-factor authentication (2FA) on your identity provider
   - Use device-bound credentials where available

3. **Monitor Transparency Log**
   - Regularly check for unauthorized publications under your namespace
   - Set up alerts for new package versions

### For Consumers

1. **Enable All Verification**
   - Set `require_tuf = true` in trust policies
   - Enable `verify_inclusion` for transparency log checks
   - Keep TUF metadata up to date

2. **Pin Dependencies**
   - Use exact version pins (`@namespace/pkg@1.2.3`)
   - Verify hashes in lockfiles match registry

3. **Regular Audits**
   - Run `lumen wares audit` before deployments
   - Review audit logs for unexpected events
   - Update dependencies promptly when vulnerabilities are found

4. **Configure Trust Policies**
   - Explicitly allow known-good namespaces
   - Deny packages with security advisories
   - Set maximum package age limits

### For Organizations

1. **Registry Mirror**
   - Host an internal registry mirror
   - Perform additional vetting before mirroring packages
   - Control package updates centrally

2. **Security Scanning**
   - Integrate `lumen wares audit` into CI/CD pipelines
   - Block builds with Critical/High vulnerabilities
   - Require manual approval for new dependencies

3. **Audit Trail**
   - Collect audit logs centrally (SIEM integration)
   - Set retention policies for compliance
   - Review logs during security incidents

4. **Incident Response**
   - Have a plan for compromised dependencies
   - Use transparency log to verify package history
   - Coordinate with registry operators on takedowns

## Next Steps

- [**Package Management**](./packages.md) — Learn about publishing and managing packages
- [**Configuration**](./configuration.md) — Configure registry and security settings
- [**CLI Reference**](./cli.md) — Complete CLI command documentation
- [**Registry Architecture**](../architecture/registry.md) — Deep dive into registry design

## Further Reading

- [The Update Framework (TUF) Specification](https://theupdateframework.io/)
- [Certificate Transparency (RFC 6962)](https://tools.ietf.org/html/rfc6962)
- [OpenID Connect Core 1.0](https://openid.net/specs/openid-connect-core-1_0.html)
- [Ed25519 Signature Scheme](https://ed25519.cr.yp.to/)
