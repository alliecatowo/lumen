import { describe, expect, it } from 'vitest';
import {
  bytesToHex,
  checkpointBody,
  consistencyProof,
  emptyRoot,
  hashChildren,
  inclusionProof,
  leafHash,
  mth,
  signCheckpoint,
  verifyCheckpoint,
  verifyConsistency,
  verifyInclusion,
} from '../src/merkle.js';
import { handleRequest } from '../src/index.js';
import { createFakeD1 } from './fake-d1.js';
import { HASH, genKey, issueCert, sign, spki } from './helpers.js';

const entryHash = (i) => (i.toString(16).padStart(2, '0')).repeat(32);
async function leaves(n) {
  return Promise.all(Array.from({ length: n }, (_, i) => leafHash(entryHash(i))));
}

describe('RFC 6962 hashing', () => {
  it('matches the well known empty-tree and empty-leaf constants', async () => {
    expect(bytesToHex(await emptyRoot())).toBe('e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
    // SHA-256(0x00): the Certificate Transparency empty leaf.
    const { createHash } = await import('node:crypto');
    expect(createHash('sha256').update(Buffer.from([0])).digest('hex')).toBe(
      '6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d',
    );
  });

  it('does not duplicate the last leaf (odd trees differ from padded trees)', async () => {
    const l = await leaves(3);
    const padded = await hashChildren(await hashChildren(l[0], l[1]), await hashChildren(l[2], l[2]));
    expect(bytesToHex(await mth(l))).not.toBe(bytesToHex(padded));
    expect(bytesToHex(await mth(l))).toBe(bytesToHex(await hashChildren(await hashChildren(l[0], l[1]), l[2])));
  });

  it('separates leaves from interior nodes', async () => {
    const l = await leaves(2);
    const node = await hashChildren(l[0], l[1]);
    // A leaf whose data equals two concatenated leaf hashes must not collide with the node.
    const fakeLeaf = await leafHash(bytesToHex(new Uint8Array([...l[0], ...l[1]]).slice(0, 32)));
    expect(bytesToHex(fakeLeaf)).not.toBe(bytesToHex(node));
  });
});

describe('inclusion proofs', () => {
  it('verify for every index of every tree size up to 17', async () => {
    const all = await leaves(17);
    for (let n = 1; n <= 17; n++) {
      const root = await mth(all, 0, n);
      for (let m = 0; m < n; m++) {
        const proof = await inclusionProof(all, m, n);
        expect(await verifyInclusion(all[m], m, n, proof, root)).toBe(true);
      }
    }
  });

  it('reject wrong leaf, wrong index, wrong root and truncated proofs', async () => {
    const all = await leaves(9);
    const root = await mth(all);
    const proof = await inclusionProof(all, 4, 9);
    expect(await verifyInclusion(all[5], 4, 9, proof, root)).toBe(false);
    expect(await verifyInclusion(all[4], 5, 9, proof, root)).toBe(false);
    expect(await verifyInclusion(all[4], 4, 9, proof, await mth(all, 0, 8))).toBe(false);
    expect(await verifyInclusion(all[4], 4, 9, proof.slice(1), root)).toBe(false);
    expect(await verifyInclusion(all[4], 9, 9, proof, root)).toBe(false);
  });
});

describe('consistency proofs', () => {
  it('verify for every pair of sizes up to 17', async () => {
    const all = await leaves(17);
    for (let n = 1; n <= 17; n++) {
      const r2 = await mth(all, 0, n);
      for (let m = 1; m <= n; m++) {
        const r1 = await mth(all, 0, m);
        const proof = await consistencyProof(all, m, n);
        expect(await verifyConsistency(m, n, r1, r2, proof)).toBe(true);
      }
    }
  });

  it('reject a rewritten history', async () => {
    const all = await leaves(10);
    const tampered = [...all];
    tampered[2] = await leafHash(entryHash(200));
    const proof = await consistencyProof(all, 6, 10);
    const r1 = await mth(all, 0, 6);
    expect(await verifyConsistency(6, 10, r1, await mth(tampered), proof)).toBe(false);
    expect(await verifyConsistency(6, 10, await mth(tampered, 0, 6), await mth(all), proof)).toBe(false);
  });
});

describe('signed checkpoints', () => {
  async function pems() {
    const kp = await genKey();
    const pk8 = Buffer.from(await crypto.subtle.exportKey('pkcs8', kp.privateKey)).toString('base64');
    return { priv: `-----BEGIN PRIVATE KEY-----\n${pk8}\n-----END PRIVATE KEY-----`, pub: await spki(kp) };
  }

  it('verifies with the public key and fails on any change', async () => {
    const { priv, pub } = await pems();
    const root = 'ab'.repeat(32);
    const sig = await signCheckpoint(priv, 5, root, 1700000000000);
    const cp = { tree_size: 5, root_hash: root, timestamp: 1700000000000, signed_tree_head: sig };
    expect(await verifyCheckpoint(pub, cp)).toBe(true);
    expect(await verifyCheckpoint(pub, { ...cp, tree_size: 6 })).toBe(false);
    expect(await verifyCheckpoint(pub, { ...cp, root_hash: 'cd'.repeat(32) })).toBe(false);
    // The old scheme (SHA-256 of the data) must not verify.
    const { createHash } = await import('node:crypto');
    const fake = createHash('sha256').update(checkpointBody(5, root, 1700000000000) + '-signed').digest('hex');
    expect(await verifyCheckpoint(pub, { ...cp, signed_tree_head: fake })).toBe(false);
  });
});

describe('log endpoints', () => {
  it('serve proofs that verify against the checkpoint root', async () => {
    const ca = await genKey();
    const user = await genKey();
    const logKey = await genKey();
    const pk8 = Buffer.from(await crypto.subtle.exportKey('pkcs8', logKey.privateKey)).toString('base64');
    const env = {
      wares_transparency_log: createFakeD1(),
      REGISTRY_API_KEY: 'k',
      CA_PUBLIC_KEY: await spki(ca),
      LOG_PRIVATE_KEY: `-----BEGIN PRIVATE KEY-----\n${pk8}\n-----END PRIVATE KEY-----`,
      LOG_PUBLIC_KEY: await spki(logKey),
    };
    const call = (path, init) => handleRequest(new Request('https://log.test' + path, init), env);

    for (let i = 0; i < 6; i++) {
      const res = await call('/api/v1/log/entries', {
        method: 'POST',
        headers: { 'X-API-Key': 'k' },
        body: JSON.stringify({
          package_name: '@a/p',
          version: `1.0.${i}`,
          content_hash: HASH,
          identity: 'github.com/a',
          certificate: await issueCert(ca, user, { subject: 'github.com/a' }),
          signature: await sign(user.privateKey, HASH),
        }),
      });
      expect(res.status).toBe(201);
    }

    const cp = await (await call('/api/v1/log/checkpoint')).json();
    expect(cp.tree_size).toBe(6);
    const pub = (await (await call('/api/v1/log/public-key')).json()).public_key;
    expect(await verifyCheckpoint(pub, cp)).toBe(true);

    const hex = (h) => Uint8Array.from(Buffer.from(h, 'hex'));
    const p = await (await call('/api/v1/log/proof/3')).json();
    expect(p.root_hash).toBe(cp.root_hash);
    expect(await verifyInclusion(hex(p.leaf_hash), 3, 6, p.proof.map(hex), hex(cp.root_hash))).toBe(true);

    const c = await (await call('/api/v1/log/consistency/2/6')).json();
    expect(await verifyConsistency(2, 6, hex(c.root1), hex(c.root2), c.proof.map(hex))).toBe(true);
    expect(c.root2).toBe(cp.root_hash);

    // historic tree size and bad input
    const old = await (await call('/api/v1/log/proof/1?tree_size=3')).json();
    expect(await verifyInclusion(hex(old.leaf_hash), 1, 3, old.proof.map(hex), hex(old.root_hash))).toBe(true);
    expect((await call('/api/v1/log/proof/9')).status).toBe(404);
    expect((await call('/api/v1/log/proof/x')).status).toBe(400);
    expect((await call('/api/v1/log/consistency/5/2')).status).toBe(404);
  });

  it('stores an unsigned checkpoint when no log key is configured', async () => {
    const ca = await genKey();
    const user = await genKey();
    const env = { wares_transparency_log: createFakeD1(), REGISTRY_API_KEY: 'k', CA_PUBLIC_KEY: await spki(ca) };
    await handleRequest(
      new Request('https://log.test/api/v1/log/entries', {
        method: 'POST',
        headers: { 'X-API-Key': 'k' },
        body: JSON.stringify({
          package_name: '@a/p',
          version: '1.0.0',
          content_hash: HASH,
          identity: 'github.com/a',
          certificate: await issueCert(ca, user, { subject: 'github.com/a' }),
          signature: await sign(user.privateKey, HASH),
        }),
      }),
      env,
    );
    const cp = await (await handleRequest(new Request('https://log.test/api/v1/log/checkpoint'), env)).json();
    expect(cp.signed_tree_head).toBe('');
  });
});
