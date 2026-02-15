# Deploy Transparency Log Worker

## Quick Deploy

```bash
cd workers/transparency-log

# 1. Deploy the worker
wrangler deploy

# 2. Add custom domain (choose ONE method):

# Method A: Via wrangler (recommended)
wrangler route publish --pattern "log.wares.lumen-lang.com/*" --script wares-transparency-log

# Method B: Via Cloudflare API (if you have API token)
curl -X POST "https://api.cloudflare.com/client/v4/zones/YOUR_ZONE_ID/workers/routes" \
  -H "Authorization: Bearer YOUR_API_TOKEN" \
  -H "Content-Type: application/json" \
  --data '{
    "pattern": "log.wares.lumen-lang.com/*",
    "script": "wares-transparency-log"
  }'

# Method C: Dashboard (easiest)
# 1. Go to https://dash.cloudflare.com → your domain → Workers Routes
# 2. Click "Add Route"
# 3. Pattern: log.wares.lumen-lang.com/*
# 4. Worker: wares-transparency-log
# 5. Click Save
```

## Manual Dashboard Steps

1. Go to [Cloudflare Dashboard](https://dash.cloudflare.com)
2. Select your domain (lumen-lang.com)
3. Click **Workers & Pages** in the left sidebar
4. Click **Add route**
5. Fill in:
   - **Route**: `log.wares.lumen-lang.com/*`
   - **Worker**: `wares-transparency-log`
   - **Environment**: Production
6. Click **Save**

## Verify

```bash
# Should return {"status":"ok","service":"wares-transparency-log"}
curl https://log.wares.lumen-lang.com/health

# Check log info
curl https://log.wares.lumen-lang.com/api/v1/log
```

## DNS Check

Make sure you have a DNS record pointing to the worker:

```bash
# Check DNS resolution
dig log.wares.lumen-lang.com

# Should show Cloudflare IPs (104.21.x.x or 172.67.x.x)
```

If DNS isn't resolving, add a CNAME record in Cloudflare DNS:
- **Name**: `log`
- **Target**: `wares-transparency-log.YOUR_SUBDOMAIN.workers.dev`
- **Proxy status**: Orange cloud (proxied)
