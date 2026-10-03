import { bytesToHex, consistencyProof, inclusionProof, leafHash, mth } from './merkle.js';

const CERT_BEGIN = '-----BEGIN WARES CERTIFICATE-----';
const CERT_SIG_BEGIN = '-----BEGIN SIGNATURE-----';
const CERT_END = '-----END WARES CERTIFICATE-----';
/** Allowed clock skew when checking certificate validity (ms). */
const CLOCK_SKEW_MS = 60 * 1000;

function b64ToBytes(b64) {
  const clean = String(b64).replace(/\s+/g, '');
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(clean)) throw new Error('invalid base64');
  return Uint8Array.from(atob(clean), (c) => c.charCodeAt(0));
}

/** Parse a base64 or PEM (`BEGIN PUBLIC KEY`) SPKI public key. */
export function parseSpki(keyText) {
  const b64 = String(keyText)
    .replace(/-----BEGIN PUBLIC KEY-----/, '')
    .replace(/-----END PUBLIC KEY-----/, '');
  return b64ToBytes(b64);
}

/**
 * Convert an ASN.1 DER ECDSA signature to the IEEE P1363 (r || s) form that
 * WebCrypto expects. Signatures already 64 bytes long are returned as is.
 */
export function normalizeEcdsaSignature(sig) {
  if (sig.length === 64) return sig;
  if (sig[0] !== 0x30) throw new Error('unrecognised signature encoding');
  let i = 2;
  if (sig[1] & 0x80) i = 2 + (sig[1] & 0x7f);
  const readInt = () => {
    if (sig[i] !== 0x02) throw new Error('bad DER integer');
    const len = sig[i + 1];
    let v = sig.slice(i + 2, i + 2 + len);
    i += 2 + len;
    while (v.length > 32 && v[0] === 0) v = v.slice(1);
    if (v.length > 32) throw new Error('integer too large');
    const out = new Uint8Array(32);
    out.set(v, 32 - v.length);
    return out;
  };
  const r = readInt();
  const s = readInt();
  const raw = new Uint8Array(64);
  raw.set(r, 0);
  raw.set(s, 32);
  return raw;
}

/**
 * Verify a log entry's package signature. Fails closed.
 *
 * 1. The certificate must carry a CA signature that verifies under
 *    `env.CA_PUBLIC_KEY` (SPKI, base64 or PEM) over the exact certificate JSON.
 * 2. The certificate must be valid at `now` and its subject must equal the
 *    entry's `identity`.
 * 3. `signature` must verify under the certificate's public key over
 *    `content_hash`.
 *
 * Returns `{ ok: true }` or `{ ok: false, reason }`.
 */
export async function verifyPackageSignature(body, env, now = Date.now()) {
  try {
    if (!env || !env.CA_PUBLIC_KEY) {
      return { ok: false, reason: 'CA_PUBLIC_KEY is not configured', misconfigured: true };
    }
    const { signature, certificate, content_hash, identity } = body;
    if ([signature, certificate, content_hash, identity].some((v) => typeof v !== 'string')) {
      return { ok: false, reason: 'signature, certificate, content_hash and identity must be strings' };
    }

    // Parse the Wares Certificate
    const lines = certificate.split('\n').map((l) => l.trim());
    const startIdx = lines.indexOf(CERT_BEGIN);
    const sigStartIdx = lines.indexOf(CERT_SIG_BEGIN);
    const endIdx = lines.indexOf(CERT_END);
    if (startIdx === -1 || sigStartIdx === -1 || endIdx === -1 || !(startIdx < sigStartIdx && sigStartIdx < endIdx)) {
      return { ok: false, reason: 'malformed certificate' };
    }
    const certJsonStr = new TextDecoder().decode(b64ToBytes(lines.slice(startIdx + 1, sigStartIdx).join('')));
    const certSig = normalizeEcdsaSignature(b64ToBytes(lines.slice(sigStartIdx + 1, endIdx).join('')));
    const certData = JSON.parse(certJsonStr);

    // 1. CA signature over the exact certificate bytes
    const caKey = await crypto.subtle.importKey(
      'spki',
      parseSpki(env.CA_PUBLIC_KEY),
      { name: 'ECDSA', namedCurve: 'P-256' },
      false,
      ['verify'],
    );
    const caOk = await crypto.subtle.verify(
      { name: 'ECDSA', hash: 'SHA-256' },
      caKey,
      certSig,
      new TextEncoder().encode(certJsonStr),
    );
    if (!caOk) return { ok: false, reason: 'certificate is not signed by the CA' };

    // 2. Validity window and identity binding
    const notBefore = Date.parse(certData.not_before);
    const notAfter = Date.parse(certData.not_after);
    if (!Number.isFinite(notBefore) || !Number.isFinite(notAfter)) {
      return { ok: false, reason: 'certificate has no validity window' };
    }
    if (now < notBefore - CLOCK_SKEW_MS || now > notAfter + CLOCK_SKEW_MS) {
      return { ok: false, reason: 'certificate is expired or not yet valid' };
    }
    if (typeof certData.subject !== 'string' || certData.subject !== identity) {
      return { ok: false, reason: 'identity does not match certificate subject' };
    }

    // 3. Package signature under the certificate's key
    const userKey = await crypto.subtle.importKey(
      'spki',
      b64ToBytes(certData.public_key),
      { name: 'ECDSA', namedCurve: 'P-256' },
      false,
      ['verify'],
    );
    const sigOk = await crypto.subtle.verify(
      { name: 'ECDSA', hash: 'SHA-256' },
      userKey,
      normalizeEcdsaSignature(b64ToBytes(signature)),
      new TextEncoder().encode(content_hash),
    );
    return sigOk ? { ok: true } : { ok: false, reason: 'package signature does not verify' };
  } catch (e) {
    return { ok: false, reason: 'signature verification failed' };
  }
}

/** Load the leaf hashes of the first `size` entries; null if the log is shorter or has gaps. */
async function loadLeaves(db, size) {
  const result = await db
    .prepare('SELECT this_hash FROM log_entries WHERE "index" < ? ORDER BY "index"')
    .bind(size)
    .all();
  const rows = result.results || [];
  if (rows.length !== size) return null;
  return Promise.all(rows.map((r) => leafHash(r.this_hash)));
}

/**
 * RFC 6962 inclusion proof for entry `targetIndex` in the tree of the first
 * `treeSize` entries (default: the whole log). Returns null if out of range.
 */
export async function generateInclusionProof(db, targetIndex, treeSize) {
  const count = (await db.prepare('SELECT COUNT(*) as count FROM log_entries').first())?.count || 0;
  const size = treeSize === undefined ? count : treeSize;
  if (!Number.isInteger(targetIndex) || targetIndex < 0 || targetIndex >= size || size > count) return null;
  const leaves = await loadLeaves(db, size);
  if (!leaves) return null;
  const root = await mth(leaves);
  return {
    index: targetIndex,
    tree_size: size,
    leaf_hash: bytesToHex(leaves[targetIndex]),
    root_hash: bytesToHex(root),
    proof: (await inclusionProof(leaves, targetIndex, size)).map(bytesToHex),
  };
}

/**
 * RFC 6962 consistency proof showing the first `size1` entries are a prefix
 * of the first `size2`. Returns null for invalid sizes.
 */
export async function generateConsistencyProof(db, size1, size2) {
  if (!Number.isInteger(size1) || !Number.isInteger(size2) || size1 < 0 || size1 > size2) return null;
  const count = (await db.prepare('SELECT COUNT(*) as count FROM log_entries').first())?.count || 0;
  if (size2 > count) return null;
  const leaves = await loadLeaves(db, size2);
  if (!leaves) return null;
  return {
    size1,
    size2,
    root1: bytesToHex(await mth(leaves, 0, size1)),
    root2: bytesToHex(await mth(leaves)),
    proof: (await consistencyProof(leaves, size1, size2)).map(bytesToHex),
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
