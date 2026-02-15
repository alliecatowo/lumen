/**
 * Verify a package signature using ECDSA P-256
 * Proves that the package content was signed by the identity in the certificate
 */
export async function verifyPackageSignature(body) {
  try {
    const { signature, certificate, content_hash } = body;

    // 1. Parse the Wares Certificate
    const lines = certificate.split('\n');
    const startIdx = lines.indexOf('-----BEGIN WARES CERTIFICATE-----');
    const sigStartIdx = lines.indexOf('-----BEGIN SIGNATURE-----');
    const endIdx = lines.indexOf('-----END WARES CERTIFICATE-----');

    if (startIdx === -1 || sigStartIdx === -1 || endIdx === -1) return false;

    const certJsonB64 = lines.slice(startIdx + 1, sigStartIdx).join('').trim();
    const certSigB64 = lines.slice(sigStartIdx + 1, endIdx).join('').trim();

    const certJsonStr = atob(certJsonB64);
    const certData = JSON.parse(certJsonStr);

    // 2. Verify CA Signature (Optional for now if CA_PUBLIC_KEY not set)
    // In production, we'd import CA_PUBLIC_KEY and call crypto.subtle.verify

    // 3. Extract user's public key (SPKI) from certificate
    const userPubKeyB64 = certData.public_key;
    const userPubKeyBuffer = Uint8Array.from(atob(userPubKeyB64), c => c.charCodeAt(0));

    const userKey = await crypto.subtle.importKey(
      'spki',
      userPubKeyBuffer,
      { name: 'ECDSA', namedCurve: 'P-256' },
      false,
      ['verify']
    );

    // 4. Verify user's signature over the content_hash
    const sigBuffer = Uint8Array.from(atob(signature), c => c.charCodeAt(0));
    const contentBuffer = new TextEncoder().encode(content_hash);

    return await crypto.subtle.verify(
      { name: 'ECDSA', hash: 'SHA-256' },
      userKey,
      sigBuffer,
      contentBuffer
    );
  } catch (e) {
    console.error('Signature verification failed:', e);
    return false;
  }
}

/**
 * Generate an inclusion proof for a log entry
 */
export async function generateInclusionProof(db, targetIndex) {
  // Simplified Merkle inclusion proof
  const result = await db.prepare(`
    SELECT "index", this_hash FROM log_entries ORDER BY "index"
  `).all();

  const hashes = result.results?.map(e => e.this_hash) || [];
  if (targetIndex >= hashes.length) return null;

  return {
    targetIndex,
    treeSize: hashes.length,
    leafHash: hashes[targetIndex],
    proof: await getPath(targetIndex, hashes)
  };
}

/**
 * Generate a consistency proof between two tree sizes
 * Proves that tree1 is a consistent prefix of tree2
 */
export async function generateConsistencyProof(db, size1, size2) {
  if (size1 > size2) return null;

  const result = await db.prepare(`
    SELECT "index", this_hash FROM log_entries WHERE "index" < ? ORDER BY "index"
  `).bind(size2).all();

  const hashes = result.results?.map(e => e.this_hash) || [];

  // Consistency proof logic (simplified for fixed-width Merkle if needed, but here we use the list of hashes)
  // Real Rekor/Trillian logic is more complex, we'll provide a chain of hashes.
  return {
    size1,
    size2,
    proof: hashes.slice(0, size2) // Placeholder for real consistency path
  };
}

async function getPath(index, hashes) {
  // Mock path generation for now
  return hashes.filter((_, i) => i !== index);
}

/**
 * Compute a SHA-256 hash of data
 */
export async function computeHash(data) {
  const encoder = new TextEncoder();
  const buffer = await crypto.subtle.digest('SHA-256', encoder.encode(data));
  return Array.from(new Uint8Array(buffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}
