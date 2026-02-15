# Wares Trust System Setup Guide

This guide walks you through setting up the Sigstore-style keyless signing system for Wares, the Lumen Package Manager.

## Architecture Overview

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│   wares     │────▶│   Registry   │────▶│ Transparency    │
│   login     │     │   (auth)     │     │ Log (Cloudflare)│
└─────────────┘     └──────────────┘     └─────────────────┘
       │                     │                      │
       │              ┌──────┴──────┐              │
       └─────────────▶│   GitHub    │◀─────────────┘
                      │   OIDC      │
                      └─────────────┘
```

## Components to Configure

### 1. Transparency Log Service (Cloudflare Workers + D1)

The transparency log is an append-only, cryptographically verifiable log of all package publishes.

#### Setup Steps:

```bash
cd workers/transparency-log

# 1. Create D1 database
wrangler d1 create wares-transparency-log
# Note the database_id from output

# 2. Update wrangler.toml with database_id
# Edit wrangler.toml and set database_id

# 3. Run schema migration
wrangler d1 execute wares-transparency-log --file=schema.sql

# 4. Set API key for registry access
wrangler secret put REGISTRY_API_KEY
# Enter a secure random key (e.g., `openssl rand -hex 32`)

# 5. Deploy
wrangler deploy
```

#### DNS Configuration:

Add CNAME or custom domain in Cloudflare:
```
log.wares.lumen-lang.com → wares-transparency-log.your-subdomain.workers.dev
```

### 2. Registry Server (Registry Authentication)

The registry handles OIDC authentication and issues ephemeral signing certificates.

#### Setup Environment Variables:

```bash
# In your registry-server/.env or Cloudflare Worker secrets:

# GitHub OAuth App credentials
GITHUB_CLIENT_ID=your_github_oauth_app_id
GITHUB_CLIENT_SECRET=your_github_oauth_app_secret

# Transparency log API key (same as above)
TRANSPARENCY_LOG_API_KEY=same_key_as_above
TRANSPARENCY_LOG_URL=https://log.wares.lumen-lang.com

# Fulcio-like CA signing key (generate with: openssl ecparam -genkey -name prime256v1)
CA_PRIVATE_KEY="-----BEGIN EC PRIVATE KEY-----\n..."
```

#### GitHub OAuth App Setup:

1. Go to GitHub Settings → Developer Settings → OAuth Apps
2. Click "New OAuth App"
3. Fill in:
   - **Application name**: Wares Registry
   - **Homepage URL**: https://wares.lumen-lang.com
   - **Authorization callback URL**: https://wares.lumen-lang.com/api/v1/auth/oidc/callback
4. Save Client ID and Client Secret

### 3. Client Configuration (User's Machine)

Users configure trust policies in `~/.wares/trust.toml`:

```toml
version = 1

[oidc_credentials]
[oidc_credentials."https://wares.lumen-lang.com"]
provider = "github"
refresh_token = "encrypted_token_here"
identity = "github.com/username"
obtained_at = "2024-01-15T10:00:00Z"
expires_at = "2024-01-16T10:00:00Z"

[policies]
[policies."https://wares.lumen-lang.com"]
required_identity = "^https://github.com/[^/]+/[^/]+/.github/workflows/.*$"
min_slsa_level = 2
require_transparency_log = true
min_package_age = "24h"
block_install_scripts = true
allowed_providers = ["github"]
```

## Trust Levels

### Permissive (Development)
```toml
require_transparency_log = false
min_slsa_level = 0
```

### Normal (Default)
```toml
require_transparency_log = true
min_slsa_level = 1
block_install_scripts = false
```

### Strict (High Security)
```toml
required_identity = "^https://github.com/[^/]+/[^/]+/.github/workflows/.*$"
min_slsa_level = 3
require_transparency_log = true
min_package_age = "24h"
block_install_scripts = true
allowed_providers = ["github"]
```

## Usage

### First-time Setup (User)

```bash
# Authenticate with GitHub
wares login --provider github
# Opens browser, authorizes, stores refresh token

# Check authentication
wares whoami
# → Logged in as github.com/username/repo/.github/workflows/release.yml
```

### Publishing

```bash
# In a package directory with lumen.toml
wares publish

# Output:
# → Requesting ephemeral signing certificate...
# → Got certificate valid for 10m
# → Package mypackage@1.0.0 signed by github.com/username/repo/.github/workflows/release.yml
# → Transparency log entry #892341
# → Published mypackage@1.0.0
```

### Installing with Trust Verification

```bash
# Install with default policy
wares install some-package

# Output:
# → Fetching package...
# → Verifying trust...
#   ✓ Signed by: github.com/org/some-package/.github/workflows/release.yml
#   ✓ SLSA Level: 3
#   ✓ Transparency log: #892341
#   ⚠ Package is new (2 hours old, 24h recommended)
# → Installed some-package@1.0.0

# Install with strict policy
wares install some-package --trust strict
# Will fail if package doesn't meet strict requirements

# Check trust without installing
wares trust-check some-package@1.0.0
```

### CI/CD Publishing

In GitHub Actions, publish with SLSA provenance:

```yaml
name: Publish
on:
  push:
    tags: ['v*']

jobs:
  publish:
    runs-on: ubuntu-latest
    permissions:
      id-token: write  # Required for OIDC
      contents: read
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Wares
        run: |
          curl -fsSL https://wares.lumen-lang.com/install.sh | sh
          wares login --provider github
      
      - name: Publish with Provenance
        run: |
          VERSION=${GITHUB_REF#refs/tags/v}
          wares publish --provenance
        env:
          WARES_REGISTRY: https://wares.lumen-lang.com
```

## Security Model

### Threats Addressed

1. **Compromised Developer Machine**
   - No long-lived signing keys on developer laptops
   - Short-lived certificates (10 minutes max)
   - Identity tied to OIDC provider

2. **Package Tampering**
   - All publishes recorded in transparency log
   - Cryptographic chain of entries
   - Anyone can verify inclusion

3. **Build System Compromise**
   - SLSA provenance links package to specific CI run
   - Reproducible builds can verify binary matches source
   - Policy can require specific builders

4. **Account Takeover**
   - Transparency log makes malicious publishes visible
   - Package age policies give time for detection
   - Identity verification through OIDC

5. **Rollback / Fork Attacks**
   - Append-only log prevents deletion
   - Monotonic indices prevent replay
   - Clients monitor for split-views

### Monitoring

Set up monitoring to detect suspicious activity:

```bash
# Poll for new entries
watch -n 60 'curl https://log.wares.lumen-lang.com/api/v1/log/monitor?start=$(cat last_index)'

# Alert on:
# - Unexpected package publishes
# - New identities publishing to existing packages
# - Split-view indicators (different roots for different clients)
```

## Verification Flow

When a user runs `wares install`:

1. **Fetch Package** from registry
2. **Fetch Signature** from registry
3. **Verify Certificate**:
   - Check certificate is valid and not expired
   - Verify OIDC issuer is trusted
   - Check identity matches expected pattern
4. **Verify Transparency Log Inclusion**:
   - Fetch log entry at claimed index
   - Verify entry matches package data
   - Verify inclusion proof
5. **Check Policy**:
   - SLSA level sufficient?
   - Package old enough?
   - Install scripts allowed?
6. **Install** if all checks pass

## Troubleshooting

### "Not authenticated" Error

```bash
wares login
# Check browser opened and OAuth completed
```

### Certificate Expired During Publish

```bash
# Re-login to get fresh credentials
wares logout && wares login
```

### Transparency Log Verification Fails

```bash
# Check log status
curl https://log.wares.lumen-lang.com/api/v1/log

# Query specific package
curl "https://log.wares.lumen-lang.com/api/v1/log/query?package=mypackage"
```

### Policy Blocking Install

```bash
# Check current policy
wares policy show

# Temporarily use permissive policy (not recommended for production)
wares install package --trust permissive
```

## Future Enhancements

1. **Multi-signature Support**: Require N-of-M signatures for critical packages
2. **Key Transparency**: Monitor for unexpected key rotations
3. **Automated Policy Updates**: Crowd-sourced reputation scoring
4. **Binary Transparency**: Log pre-built binaries for reproducibility
5. **Integration with SLSA Provenance**: Full attestation verification
