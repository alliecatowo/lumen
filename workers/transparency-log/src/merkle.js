/**
 * RFC 6962 / RFC 9162 Merkle tree over the log's entry hashes.
 *
 * - leaf hash  = SHA-256(0x00 || entry_hash_bytes)   (entry_hash is the 32 byte chain hash)
 * - node hash  = SHA-256(0x01 || left || right)
 * - empty tree = SHA-256("")
 *
 * Domain separation between leaves and nodes rules out second-preimage
 * attacks, and the tree is never padded by duplicating the last leaf.
 * All functions here operate on arrays of Uint8Array leaf hashes.
 */

const enc = new TextEncoder();

export function hexToBytes(hex) {
  if (!/^([0-9a-f]{2})*$/i.test(hex)) throw new Error('invalid hex');
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.substr(i * 2, 2), 16);
  return out;
}

export function bytesToHex(bytes) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

async function sha256(...parts) {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const buf = new Uint8Array(total);
  let off = 0;
  for (const p of parts) {
    buf.set(p, off);
    off += p.length;
  }
  return new Uint8Array(await crypto.subtle.digest('SHA-256', buf));
}

const LEAF_PREFIX = new Uint8Array([0x00]);
const NODE_PREFIX = new Uint8Array([0x01]);

/** Hash of the empty tree. */
export function emptyRoot() {
  return sha256(new Uint8Array(0));
}

/** RFC 6962 leaf hash of a log entry hash (hex string). */
export function leafHash(entryHashHex) {
  return sha256(LEAF_PREFIX, hexToBytes(entryHashHex));
}

export function hashChildren(left, right) {
  return sha256(NODE_PREFIX, left, right);
}

/** Largest power of two strictly less than n (n >= 2). */
function split(n) {
  let k = 1;
  while (k * 2 < n) k *= 2;
  return k;
}

/** Merkle Tree Hash of leaves[lo, hi). */
export async function mth(leaves, lo = 0, hi = leaves.length) {
  const n = hi - lo;
  if (n === 0) return emptyRoot();
  if (n === 1) return leaves[lo];
  const k = split(n);
  return hashChildren(await mth(leaves, lo, lo + k), await mth(leaves, lo + k, hi));
}

/** Audit path for leaf `m` in the tree of the first `n` leaves (RFC 6962 2.1.1). */
export async function inclusionProof(leaves, m, n = leaves.length) {
  if (!(m >= 0 && m < n && n <= leaves.length)) throw new RangeError('index out of range');
  const path = async (idx, lo, hi) => {
    if (hi - lo === 1) return [];
    const k = split(hi - lo);
    if (idx < k) return [...(await path(idx, lo, lo + k)), await mth(leaves, lo + k, hi)];
    return [...(await path(idx - k, lo + k, hi)), await mth(leaves, lo, lo + k)];
  };
  return path(m, 0, n);
}

/** Consistency proof between the first `m` leaves and the first `n` leaves (RFC 6962 2.1.2). */
export async function consistencyProof(leaves, m, n = leaves.length) {
  if (!(m >= 0 && m <= n && n <= leaves.length)) throw new RangeError('size out of range');
  if (m === 0 || m === n) return [];
  const sub = async (mm, lo, hi, complete) => {
    const size = hi - lo;
    if (mm === size) return complete ? [] : [await mth(leaves, lo, hi)];
    const k = split(size);
    if (mm <= k) return [...(await sub(mm, lo, lo + k, complete)), await mth(leaves, lo + k, hi)];
    return [...(await sub(mm - k, lo + k, hi, false)), await mth(leaves, lo, lo + k)];
  };
  return sub(m, 0, n, true);
}

const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);

/** Verify an inclusion proof (RFC 9162 2.1.3.2). */
export async function verifyInclusion(leaf, index, treeSize, proof, root) {
  if (index >= treeSize) return false;
  let fn = index;
  let sn = treeSize - 1;
  let r = leaf;
  for (const p of proof) {
    if (sn === 0) return false;
    if ((fn & 1) === 1 || fn === sn) {
      r = await hashChildren(p, r);
      if ((fn & 1) === 0) {
        while ((fn & 1) === 0 && fn !== 0) {
          fn >>= 1;
          sn >>= 1;
        }
      }
    } else {
      r = await hashChildren(r, p);
    }
    fn >>= 1;
    sn >>= 1;
  }
  return sn === 0 && equal(r, root);
}

/** Verify a consistency proof (RFC 9162 2.1.4.2). */
export async function verifyConsistency(size1, size2, root1, root2, proof) {
  if (size1 > size2) return false;
  if (size1 === size2) return proof.length === 0 && equal(root1, root2);
  if (size1 === 0) return proof.length === 0;
  if (proof.length === 0) return false;
  const path = (size1 & (size1 - 1)) === 0 ? [root1, ...proof] : [...proof];
  let fn = size1 - 1;
  let sn = size2 - 1;
  while ((fn & 1) === 1) {
    fn >>= 1;
    sn >>= 1;
  }
  let fr = path[0];
  let sr = path[0];
  for (const c of path.slice(1)) {
    if (sn === 0) return false;
    if ((fn & 1) === 1 || fn === sn) {
      fr = await hashChildren(c, fr);
      sr = await hashChildren(c, sr);
      if ((fn & 1) === 0) {
        while ((fn & 1) === 0 && fn !== 0) {
          fn >>= 1;
          sn >>= 1;
        }
      }
    } else {
      sr = await hashChildren(sr, c);
    }
    fn >>= 1;
    sn >>= 1;
  }
  return sn === 0 && equal(fr, root1) && equal(sr, root2);
}

// ---------------------------------------------------------------------------
// Signed checkpoints
// ---------------------------------------------------------------------------

export const CHECKPOINT_ORIGIN = 'wares-transparency-log';

/** The exact bytes a checkpoint signature covers. */
export function checkpointBody(treeSize, rootHashHex, timestamp) {
  return `${CHECKPOINT_ORIGIN}\n${treeSize}\n${rootHashHex}\n${timestamp}\n`;
}

function pemBody(pem) {
  return pem.replace(/-----(BEGIN|END)[^-]+-----/g, '').replace(/\s+/g, '');
}

function b64Bytes(b64) {
  return Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
}

/** Sign a checkpoint with the log key (PKCS#8 PEM, ECDSA P-256). Returns base64 (P1363). */
export async function signCheckpoint(privateKeyPem, treeSize, rootHashHex, timestamp) {
  const key = await crypto.subtle.importKey(
    'pkcs8',
    b64Bytes(pemBody(privateKeyPem)),
    { name: 'ECDSA', namedCurve: 'P-256' },
    false,
    ['sign'],
  );
  const sig = new Uint8Array(
    await crypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, key, enc.encode(checkpointBody(treeSize, rootHashHex, timestamp))),
  );
  return btoa(String.fromCharCode(...sig));
}

/** Verify a checkpoint signature against the log's SPKI public key (PEM or base64). */
export async function verifyCheckpoint(publicKeySpki, checkpoint) {
  try {
    const key = await crypto.subtle.importKey(
      'spki',
      b64Bytes(pemBody(publicKeySpki)),
      { name: 'ECDSA', namedCurve: 'P-256' },
      false,
      ['verify'],
    );
    return await crypto.subtle.verify(
      { name: 'ECDSA', hash: 'SHA-256' },
      key,
      b64Bytes(checkpoint.signed_tree_head),
      enc.encode(checkpointBody(checkpoint.tree_size, checkpoint.root_hash, checkpoint.timestamp)),
    );
  } catch {
    return false;
  }
}
