# Wares Registry Setup Guide

## Quick Start

### 1. Create GitHub OAuth App

Go to: https://github.com/settings/applications/new

Fill in:
- **Application name**: Wares Registry
- **Homepage URL**: http://localhost:3000
- **Authorization callback URL**: http://localhost:3000/api/v1/auth/oidc/callback/test

Click **Register application**

Copy the **Client ID** and **Client Secret**

### 2. Update .env

Edit `.env` and fill in your GitHub credentials:

```bash
export GITHUB_CLIENT_ID=your_actual_client_id
export GITHUB_CLIENT_SECRET=your_actual_client_secret
```

### 3. Start the Registry

```bash
cd registry-server
source ../.env
cargo run --release
```

You should see:
```
🚀 Wares Registry Server listening on 0.0.0.0:3000
🔐 OIDC authentication enabled
📜 Certificate Authority enabled
📝 Transparency log: https://wares-transparency-log.alliecatowo.workers.dev
```

### 4. Test Authentication

In another terminal:

```bash
# Build the wares CLI
cd rust/lumen-cli
cargo build --release --bin wares

# Test login
./target/release/wares login --provider github
```

This should:
1. Open your browser to GitHub
2. Ask you to authorize the app
3. Return to the callback URL
4. Show "Authentication successful!"

### 5. Test Publishing

```bash
cd test-package
../target/release/wares publish --dry-run
```

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│   wares     │────▶│   Registry   │────▶│  Transparency   │
│   login     │     │   :3000      │     │     Log         │
└─────────────┘     └──────────────┘     └─────────────────┘
       │                     │
       │              ┌──────┴──────┐
       └─────────────▶│   GitHub    │
                      │   OAuth     │
                      └─────────────┘
```

## API Endpoints

### Authentication
- `POST /api/v1/auth/oidc/login` - Start OAuth flow
- `GET /api/v1/auth/oidc/callback/:session_id` - OAuth callback
- `POST /api/v1/auth/oidc/token/:session_id` - Get tokens
- `POST /api/v1/auth/cert` - Request signing certificate

### Packages
- `POST /v1/packages` - Publish package
- `GET /v1/packages/:name` - Get package info
- `GET /v1/packages/:name/:version` - Download tarball
- `DELETE /v1/packages/:name/:version` - Yank version
- `GET /v1/search?q=query` - Search packages

## Troubleshooting

### "Invalid redirect_uri"
Make sure the callback URL in your GitHub OAuth app matches exactly.

### "Session not found"
The session expired (10 minute timeout). Try logging in again.

### Port 3000 already in use
Change the port: `export PORT=3001`

## Production Deployment

For production, you should:
1. Use HTTPS
2. Set proper GitHub OAuth callback URL (e.g., https://wares.lumen-lang.com/api/v1/auth/oidc/callback/test)
3. Add GitLab and Google OAuth apps
4. Set up persistent storage (R2/S3)
5. Configure CloudFront CDN
