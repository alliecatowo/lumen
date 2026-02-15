# Lumen Package Registry Hosting Guide

This guide explains how to set up and host a Lumen package registry.

## Overview

The Lumen package manager uses a **static, content-addressed registry** that can be hosted on any static file server (S3, CloudFlare R2, GitHub Pages, nginx, etc.).

## Registry Structure

A complete registry has this structure:

```
registry/
├── index.json                    # Global package index
├── packages/
│   └── @scope/                   # Namespaced packages (optional)
│       └── package-name/
│           ├── index.json        # Package version listing
│           ├── 1.0.0.json        # Version metadata (signed)
│           └── 1.0.1.json
├── packages/
│   └── simple-package/           # Non-namespaced packages
│       ├── index.json
│       └── 0.1.0.json
├── artifacts/
│   └── sha256/
│       └── ab/
│           └── c1234...tar       # Content-addressed tarballs
└── signatures/                   # Optional: detached signatures
    └── @scope/
        └── package/
            └── 1.0.0.sig.json
```

## Step 1: Create Registry Files

### Option A: Use the `lumen pkg publish` command (once implemented)

```bash
# In your package directory
lumen pkg publish --registry file:///path/to/registry
```

### Option B: Manual creation (current approach)

1. **Create the global index** (`index.json`):
```json
{
  "name": "my-lumen-registry",
  "version": "1.0.0",
  "updated_at": "2024-01-15T10:30:00Z",
  "package_count": 2,
  "packages": [
    {
      "name": "my-utils",
      "latest": "1.0.0",
      "description": "Utility functions"
    }
  ]
}
```

2. **Create package index** (`packages/my-utils/index.json`):
```json
{
  "name": "my-utils",
  "versions": ["0.1.0", "1.0.0"],
  "latest": "1.0.0",
  "yanked": {},
  "prereleases": [],
  "description": "Utility functions",
  "categories": ["utilities"]
}
```

3. **Create version metadata** (`packages/my-utils/1.0.0.json`):
```json
{
  "name": "my-utils",
  "version": "1.0.0",
  "deps": {
    "@org/http": "^1.0.0"
  },
  "artifacts": [
    {
      "kind": "tar",
      "url": "artifacts/sha256/ab/c1234...tar",
      "hash": "sha256:abc123...",
      "size": 10240
    }
  ],
  "integrity": {
    "manifest_hash": "sha256:def456..."
  },
  "license": "MIT",
  "description": "Utility functions"
}
```

## Step 2: Prepare Package Artifacts

For each package version:

1. **Create a tarball** of the package source:
```bash
tar -czvf my-utils-1.0.0.tar -C my-utils/ .
```

2. **Compute the SHA-256 hash**:
```bash
sha256sum my-utils-1.0.0.tar
# abc123def456...  my-utils-1.0.0.tar
```

3. **Store with content-addressed path**:
```
artifacts/sha256/ab/c123def456...tar
```

## Step 3: Host the Registry

### Option A: Local File System (for development)
```bash
export LUMEN_REGISTRY=file:///path/to/registry
```

### Option B: Static HTTP Server (nginx, Apache)
```nginx
server {
    listen 80;
    server_name registry.lumen.sh;
    
    root /var/www/registry;
    
    location / {
        try_files $uri $uri/ =404;
        add_header Access-Control-Allow-Origin *;
        add_header Cache-Control "public, max-age=3600";
    }
    
    location /artifacts/ {
        add_header Cache-Control "public, max-age=31536000, immutable";
    }
}
```

### Option C: S3 / CloudFlare R2
```bash
# Create bucket
aws s3 mb s3://lumen-registry

# Sync registry files
aws s3 sync ./registry s3://lumen-registry/ --acl public-read

# Set CORS
aws s3api put-bucket-cors --bucket lumen-registry --cors-configuration file://cors.json
```

`cors.json`:
```json
{
  "CORSRules": [{
    "AllowedOrigins": ["*"],
    "AllowedMethods": ["GET", "HEAD"],
    "AllowedHeaders": ["*"],
    "MaxAgeSeconds": 3600
  }]
}
```

### Option D: GitHub Pages
1. Create a `gh-pages` branch
2. Push registry files
3. Enable GitHub Pages in repo settings
4. URL becomes: `https://your-org.github.io/lumen-registry`

## Step 4: Configure Lumen to Use Your Registry

### Environment Variable
```bash
export LUMEN_REGISTRY=https://registry.lumen.sh
# or for local
export LUMEN_REGISTRY=file:///path/to/registry
```

### In lumen.toml
```toml
[registry]
default = "https://registry.lumen.sh"

[registry.registries]
# Named alternative registries
local = "file:///path/to/registry"
staging = "https://staging.lumen.sh"
```

## Step 5: CI/CD for Registry Updates (Optional)

### GitHub Actions Example
```yaml
name: Publish Package

on:
  push:
    tags:
      - 'v*'

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Build package
        run: |
          tar -czvf package.tar src/
          echo "SHA=$(sha256sum package.tar | cut -d' ' -f1)" >> $GITHUB_ENV
          
      - name: Update registry
        run: |
          # Clone registry repo
          git clone https://github.com/org/lumen-registry
          
          # Add new version
          # ... update JSON files ...
          
          # Push changes
          cd lumen-registry
          git add .
          git commit -m "Add my-package@1.0.0"
          git push
```

## What I (the AI) Cannot Do

1. **Create actual packages** - You need to have Lumen source code to package
2. **Set up hosting** - You need access to your infrastructure
3. **Configure DNS** - `registry.lumen.sh` needs DNS records
4. **Set up SSL/TLS** - HTTPS certificates for the registry domain
5. **Publish to S3/CDN** - You need AWS/cloud credentials
6. **Sign packages** - Requires your private keys

## Quick Start: Local Development Registry

```bash
# 1. Create registry directory
mkdir -p /tmp/lumen-registry/{packages,artifacts/sha256}

# 2. Create minimal index
cat > /tmp/lumen-registry/index.json << 'EOF'
{
  "name": "local-dev",
  "version": "1.0.0",
  "packages": []
}
EOF

# 3. Point Lumen to it
export LUMEN_REGISTRY=file:///tmp/lumen-registry

# 4. Now you can use path dependencies
# In lumen.toml:
# [dependencies]
# my-lib = { path = "../my-lib" }
```

## Need Help?

- See `/docs/registry-spec.md` for full specification
- See `/rust/lumen-cli/src/registry.rs` for implementation
- Run `lumen pkg init` to create a new package
