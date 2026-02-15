# Deploy Transparency Log Worker

## Deployed!

✅ Worker: `wares-transparency-log`  
✅ URL: https://wares-transparency-log.alliecatowo.workers.dev  
✅ Custom Domain: https://logs.wares.lumen-lang.com (added via dashboard)

## Verify

```bash
# Health check
curl https://logs.wares.lumen-lang.com/health
# → {"status":"ok","service":"wares-transparency-log"}

# Log info
curl https://logs.wares.lumen-lang.com/api/v1/log
# → {"tree_size":0,"root_hash":null,...}
```

## Environment Variable

Set this in your shell or CI:

```bash
export WARES_LOG_URL=https://logs.wares.lumen-lang.com
```

Or it defaults to that URL in the client.

## Test with D1

```bash
# Run schema migration
wrangler d1 execute wares-transparency-log --file=schema.sql --remote

# Check entries
wrangler d1 execute wares-transparency-log --command="SELECT * FROM log_entries LIMIT 5" --remote
```
