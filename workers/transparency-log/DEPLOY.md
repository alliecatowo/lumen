# Transparency Log - DEPLOYED ✅

## Status

| Component | Status | URL |
|-----------|--------|-----|
| Worker | ✅ Deployed | https://wares-transparency-log.alliecatowo.workers.dev |
| D1 Database | ✅ Migrated | wares-transparency-log |
| Custom Domain | ⏳ DNS Propagating | https://logs.wares.lumen-lang.com |

## Test Results

```bash
# Health check
GET https://wares-transparency-log.alliecatowo.workers.dev/health
→ {"status":"ok","service":"wares-transparency-log"}

# Log info  
GET https://wares-transparency-log.alliecatowo.workers.dev/api/v1/log
→ {"tree_size":1,"root_hash":"c21120...",...}

# Add entry (requires API key)
POST https://wares-transparency-log.alliecatowo.workers.dev/api/v1/log/entries
→ {"inserted":true,"index":0,"uuid":"918c20b2..."}

# Query entries
GET https://wares-transparency-log.alliecatowo.workers.dev/api/v1/log/query?package=test-package
→ {"entries":[...],"total":1,...}

# Get entry by index
GET https://wares-transparency-log.alliecatowo.workers.dev/api/v1/log/entries/0
→ Full entry with hash chain
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/api/v1/log` | GET | Get log info (tree size, root hash) |
| `/api/v1/log/entries` | POST | Add entry (requires X-API-Key) |
| `/api/v1/log/entries/:index` | GET | Get entry by index |
| `/api/v1/log/query` | GET | Query entries by package/version/identity |
| `/api/v1/log/verify/:index` | POST | Verify inclusion proof |
| `/api/v1/log/checkpoint` | GET | Get signed tree head |

## Configuration

```bash
# Environment variables for registry server
export TRANSPARENCY_LOG_URL=https://wares-transparency-log.alliecatowo.workers.dev
export TRANSPARENCY_LOG_API_KEY=b9326424bd8ae579aa0f815c310bd2f14667701116fa6068dbef3d23250954c4

# Or once custom domain is ready:
export TRANSPARENCY_LOG_URL=https://logs.wares.lumen-lang.com
```

## Custom Domain Status

The custom domain `logs.wares.lumen-lang.com` may take a few minutes to propagate.

Check with:
```bash
dig logs.wares.lumen-lang.com
curl https://logs.wares.lumen-lang.com/health
```
