#!/bin/bash
# Generate CA key pair for Wares Registry
# This generates an ECDSA P-256 key pair for signing ephemeral certificates.

set -e

echo "🔐 Generating Wares CA Key Pair..."

# 1. Generate Private Key (ECDSA P-256)
openssl ecparam -genkey -name prime256v1 -out ca-key.pem

# 2. Generate Self-Signed Certificate
openssl req -new -x509 -key ca-key.pem -out ca-cert.pem -days 3650 \
  -subj "/CN=Wares Registry CA/O=Lumen Language/C=US"

echo "✅ Generated:"
echo "  - ca-key.pem (Private Key - KEEP SECRET)"
echo "  - ca-cert.pem (Public Certificate)"
echo ""
echo "🚀 To deploy to Cloudflare Worker:"
echo ""
echo "  wrangler secret put CA_PRIVATE_KEY < ca-key.pem"
echo "  wrangler secret put CA_CERTIFICATE < ca-cert.pem"
echo ""
