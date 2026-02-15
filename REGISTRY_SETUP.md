# Wares Registry Setup Guide

## Production Setup (wares.lumen-lang.com)

### 1. Create GitHub OAuth App

Go to: https://github.com/settings/applications/new

Fill in:
- **Application name**: `Wares Registry`
- **Homepage URL**: `https://wares.lumen-lang.com`
- **Authorization callback URL**: `https://wares.lumen-lang.com/api/v1/auth/oidc/callback/test`
  - Note: The `/test` at the end is a placeholder - the actual session ID will be appended dynamically

Click **Register application**

Copy the **Client ID** and **Client Secret**

### 2. Update .env

Edit `.env` and fill in your credentials:

```bash
# GitHub OAuth App Credentials
export GITHUB_CLIENT_ID=your_actual_client_id
export GITHUB_CLIENT_SECRET=your_actual_client_secret

# Registry Base URL (your production domain)
export BASE_URL=https://wares.lumen-lang.com

# Transparency Log (already set up)
export TRANSPARENCY_LOG_URL=https://wares-transparency-log.alliecatowo.workers.dev
export TRANSPARENCY_LOG_API_KEY=b9326424bd8ae579aa0f815c310bd2f14667701116fa6068dbef3d23250954c4

# R2/S3 Storage (for package storage)
export R2_ACCESS_KEY=your_r2_access_key
export R2_SECRET_KEY=your_r2_secret_key
export R2_BUCKET=wares-registry
export R2_ENDPOINT=https://your-account.r2.cloudflarestorage.com
```

### 3. Deploy Registry

Option A: Cloudflare Workers/Pages
```bash
cd registry-server
wrangler deploy
```

Option B: Self-hosted
```bash
cd registry-server
cargo build --release
source ../.env
./target/release/lumen-registry-server
```

### 4. Test Authentication

```bash
# Configure wares CLI to use production registry
export WARES_REGISTRY=https://wares.lumen-lang.com

# Build and test
cd rust/lumen-cli
cargo build --release --bin wares
./target/release/wares login --provider github
```

## Development Setup (localhost)

For local development:

```bash
# 1. Create GitHub OAuth app with localhost callback:
# Authorization callback URL: http://localhost:3000/api/v1/auth/oidc/callback/test

# 2. Update .env
export BASE_URL=http://localhost:3000

# 3. Run registry
cd registry-server
cargo run

# 4. In another terminal, test:
cd rust/lumen-cli
cargo build --release --bin wares
./target/release/wares login --provider github
```

## How OAuth Flow Works

```
1. wares login → POST /api/v1/auth/oidc/login
                 ↓
2. Registry creates session, generates callback URL:
   https://wares.lumen-lang.com/api/v1/auth/oidc/callback/{session_id}
                 ↓
3. Registry returns GitHub auth URL with callback
                 ↓
4. Browser opens GitHub, user authorizes
                 ↓
5. GitHub redirects to callback URL with code+state
                 ↓
6. Registry exchanges code for access token
                 ↓
7. wares CLI polls for completion, gets tokens
                 ↓
8. wares CLI requests ephemeral certificate
                 ↓
9. wares CLI signs package, publishes to registry
                 ↓
10. Registry verifies signature, submits to transparency log
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/auth/oidc/login` | POST | Start OAuth flow |
| `/api/v1/auth/oidc/callback/:session_id` | GET | OAuth callback (GitHub redirects here) |
| `/api/v1/auth/oidc/token/:session_id` | POST | Get tokens after callback |
| `/api/v1/auth/cert` | POST | Request ephemeral signing certificate |
| `/v1/packages` | POST | Publish package (with signature) |
| `/v1/packages/:name` | GET | Get package info |
| `/v1/packages/:name/:version` | GET | Download tarball |
| `/v1/search` | GET | Search packages |

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `BASE_URL` | Public URL of registry | `http://localhost:3000` |
| `GITHUB_CLIENT_ID` | GitHub OAuth app ID | - |
| `GITHUB_CLIENT_SECRET` | GitHub OAuth secret | - |
| `TRANSPARENCY_LOG_URL` | Transparency log URL | - |
| `TRANSPARENCY_LOG_API_KEY` | API key for log | - |
| `PORT` | Server port | `3000` |

## Troubleshooting

### "redirect_uri mismatch"
Make sure your GitHub OAuth app's callback URL matches your BASE_URL + `/api/v1/auth/oidc/callback/{session_id}`

### "Session not found"
The session expired (10 minute timeout) or the callback URL doesn't match.

### "Invalid state parameter"
The state parameter from GitHub doesn't match the session state - possible CSRF attempt or session mismatch.
