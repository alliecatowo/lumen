#!/bin/bash
# Generate CA key pair for Wares Registry
# This generates an ECDSA P-256 key pair for signing ephemeral certificates.

set -e

echo "🔐 Generating Wares CA Key Pair..."

# 1. Generate Private Key (ECDSA P-256, PKCS#8 as WebCrypto importKey requires)
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out ca-key.pem

# 1b. Public key (SPKI) for the transparency log's CA_PUBLIC_KEY
openssl pkey -in ca-key.pem -pubout -out ca-pub.pem

# 2. Generate Self-Signed Certificate
openssl req -new -x509 -key ca-key.pem -out ca-cert.pem -days 3650 \
  -subj "/CN=Wares Registry CA/O=Lumen Language/C=US"

echo "✅ Generated:"
echo "  - ca-key.pem (Private Key - KEEP SECRET)"
echo "  - ca-cert.pem (Public Certificate)"
echo "  - ca-pub.pem (SPKI public key)"
echo ""
echo "🚀 To deploy to Cloudflare Worker:"
echo ""
echo "  wrangler secret put CA_PRIVATE_KEY < ca-key.pem"
echo "  wrangler secret put CA_CERTIFICATE < ca-cert.pem"
echo "  (in workers/transparency-log)  wrangler secret put CA_PUBLIC_KEY < ca-pub.pem"
echo ""
