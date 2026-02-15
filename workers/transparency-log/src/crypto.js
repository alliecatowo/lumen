/**
 * Cryptographic utilities for the transparency log
 */

/**
 * Verify a package signature (placeholder for real verification)
 * In production, this would verify the OIDC certificate and signature
 */
export async function verifyPackageSignature(body) {
  // TODO: Implement real signature verification
  // 1. Parse the certificate
  // 2. Extract the public key
  // 3. Verify the signature over the content hash
  
  // For now, just check that fields are present
  return !!(body.signature && body.certificate);
}

/**
 * Generate an inclusion proof for a log entry
 * This proves that a specific entry is included in the log
 */
export async function generateInclusionProof(db, targetIndex) {
  // Get all entries up to and including target
  const result = await db.prepare(`
    SELECT index, this_hash FROM log_entries WHERE index <= ? ORDER BY index
  `).bind(targetIndex).all();
  
  const entries = result.results || [];
  if (entries.length === 0) {
    return null;
  }

  // Build Merkle proof
  const proof = [];
  let currentIndex = targetIndex;
  
  // This is a simplified proof - real implementation would build proper Merkle path
  for (let i = entries.length - 1; i >= 0; i--) {
    if (i !== currentIndex) {
      proof.push({
        index: entries[i].index,
        hash: entries[i].this_hash,
      });
    }
  }

  return {
    targetIndex,
    treeSize: entries.length,
    proof,
  };
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

/**
 * Verify an inclusion proof
 * @param {number} targetIndex - Index of entry to verify
 * @param {string} targetHash - Hash of entry to verify
 * @param {number} treeSize - Size of the tree
 * @param {string} rootHash - Expected root hash
 * @param {Array} proof - Inclusion proof path
 */
export async function verifyInclusionProof(targetIndex, targetHash, treeSize, rootHash, proof) {
  // Recompute root hash from proof
  let computedHash = targetHash;
  let index = targetIndex;

  for (const node of proof) {
    if (index % 2 === 0) {
      // Target is left child
      computedHash = await hashPair(computedHash, node.hash);
    } else {
      // Target is right child
      computedHash = await hashPair(node.hash, computedHash);
    }
    index = Math.floor(index / 2);
  }

  return computedHash === rootHash;
}

async function hashPair(left, right) {
  const data = left + right;
  const encoder = new TextEncoder();
  const buffer = await crypto.subtle.digest('SHA-256', encoder.encode(data));
  return Array.from(new Uint8Array(buffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}
