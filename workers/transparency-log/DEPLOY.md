# Deploy Transparency Log Worker

## The Issue

`log.wares.lumen-lang.com` is a sub-subdomain (log → wares → lumen-lang), which causes TLS certificate issues.

**Solution**: Use `wares-log.lumen-lang.com` instead (flat structure).

## Deploy

```bash
cd workers/transparency-log

# Update the zone_id in wrangler.toml first!
vim wrangler.toml
# Change zone_id to your lumen-lang.com zone ID

# Deploy
wrangler deploy
```

## Add Custom Domain (Choose One)

### Method 1: Dashboard (Easiest)

1. Go to https://dash.cloudflare.com
2. Click **Workers & Pages** (sidebar)
3. Click **wares-transparency-log**
4. Click **Settings** tab
5. Click **Triggers**
6. Click **Add Custom Domain**
7. Enter: `wares-log.lumen-lang.com`
8. Click **Add Domain**

Done! Cloudflare handles the TLS certificate automatically.

### Method 2: Update wrangler.toml

```toml
[[custom_domains]]
domain = "wares-log.lumen-lang.com"
zone_id = "YOUR_ZONE_ID_HERE"  # Get from dash.cloudflare.com → lumen-lang.com → Overview (right sidebar)
```

Then: `wrangler deploy`

### Method 3: Cloudflare API

```bash
# Get your zone ID from the dashboard
ZONE_ID="your-zone-id"
AUTH_TOKEN="your-api-token"

curl -X POST "https://api.cloudflare.com/client/v4/zones/$ZONE_ID/workers/domains" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  --data '{
    "environment": "production",
    "hostname": "wares-log.lumen-lang.com",
    "service": "wares-transparency-log",
    "zone_id": "'"$ZONE_ID"'"
  }'
```

## Verify

```bash
# Should return {"status":"ok","service":"wares-transparency-log"}
curl https://wares-log.lumen-lang.com/health

# Check log info
curl https://wares-log.lumen-lang.com/api/v1/log
```

## Finding Your Zone ID

```bash
# In your terminal with wrangler logged in
wrangler whoami

# Or check the dashboard:
# dash.cloudflare.com → lumen-lang.com → Overview → API section (right side)
```

## Update Client Config

Once deployed, update the transparency log URL:

```bash
# In TRUST_SETUP.md or your client config
# Change from: https://log.wares.lumen-lang.com
# To:          https://wares-log.lumen-lang.com
```
