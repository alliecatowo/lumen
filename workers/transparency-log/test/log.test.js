import { beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { handleRequest, intParam, validateEntry } from '../src/index.js';
import { normalizeEcdsaSignature, verifyPackageSignature } from '../src/crypto.js';
import { createFakeD1 } from './fake-d1.js';
import { HASH, genKey, issueCert, sign, spki } from './helpers.js';

const API_KEY = 'test-registry-key';
let ca, user, other, env;

beforeAll(async () => {
  ca = await genKey();
  user = await genKey();
  other = await genKey();
});

beforeEach(async () => {
  env = {
    wares_transparency_log: createFakeD1(),
    REGISTRY_API_KEY: API_KEY,
    CA_PUBLIC_KEY: await spki(ca),
  };
});

async function goodBody(over = {}) {
  const identity = over.identity ?? 'github.com/alice';
  return {
    package_name: '@alice/pkg',
    version: '1.0.0',
    content_hash: HASH,
    identity,
    certificate: await issueCert(ca, user, { subject: identity }),
    signature: await sign(user.privateKey, HASH),
    ...over,
  };
}

const post = (body, key = API_KEY) =>
  handleRequest(
    new Request('https://log.test/api/v1/log/entries', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...(key ? { 'X-API-Key': key } : {}) },
      body: JSON.stringify(body),
    }),
    env,
  );
const get = (path) => handleRequest(new Request('https://log.test' + path), env);

describe('POST /log/entries authentication', () => {
  it('rejects a missing or wrong API key', async () => {
    expect((await post(await goodBody(), null)).status).toBe(401);
    expect((await post(await goodBody(), 'nope')).status).toBe(401);
  });
  it('rejects everything when REGISTRY_API_KEY is unset', async () => {
    delete env.REGISTRY_API_KEY;
    expect((await post(await goodBody(), 'undefined')).status).toBe(401);
    expect((await post(await goodBody(), '')).status).toBe(401);
  });
});

describe('signature verification', () => {
  it('accepts a CA-issued certificate with a matching identity and signature', async () => {
    const res = await post(await goodBody());
    expect(res.status).toBe(201);
    expect((await res.json()).index).toBe(0);
  });

  it('rejects a self-made certificate that the CA did not sign (forgery)', async () => {
    const fake = await genKey();
    const identity = 'github.com/alliecatowo';
    const body = await goodBody({
      identity,
      certificate: await issueCert(fake, user, { subject: identity }),
    });
    const res = await post(body);
    expect(res.status).toBe(400);
    expect((await res.json()).error).toMatch(/not signed by the CA/);
    expect((await (await get('/api/v1/log')).json()).tree_size).toBe(0);
  });

  it('rejects an identity that differs from the certificate subject', async () => {
    const body = await goodBody({ identity: 'github.com/mallory' });
    body.certificate = await issueCert(ca, user, { subject: 'github.com/alice' });
    const res = await post(body);
    expect(res.status).toBe(400);
    expect((await res.json()).error).toMatch(/subject/);
  });

  it('rejects expired and not-yet-valid certificates', async () => {
    const identity = 'github.com/alice';
    for (const win of [
      { notBefore: Date.now() - 3600_000, notAfter: Date.now() - 1800_000 },
      { notBefore: Date.now() + 3600_000, notAfter: Date.now() + 7200_000 },
    ]) {
      const res = await post(await goodBody({ certificate: await issueCert(ca, user, { subject: identity, ...win }) }));
      expect(res.status).toBe(400);
    }
  });

  it("rejects a signature made with a different key than the certificate's", async () => {
    const res = await post(await goodBody({ signature: await sign(other.privateKey, HASH) }));
    expect(res.status).toBe(400);
  });

  it('rejects a signature over a different content hash', async () => {
    const res = await post(await goodBody({ signature: await sign(user.privateKey, 'sha256:' + 'cd'.repeat(32)) }));
    expect(res.status).toBe(400);
  });

  it('rejects placeholder values', async () => {
    expect((await post(await goodBody({ signature: 'none', certificate: 'none' }))).status).toBe(400);
  });

  it('fails closed (503) when CA_PUBLIC_KEY is not configured', async () => {
    delete env.CA_PUBLIC_KEY;
    expect((await post(await goodBody())).status).toBe(503);
  });

  it('accepts DER-encoded ECDSA signatures', () => {
    const r = new Uint8Array(32).fill(1);
    const s = new Uint8Array(32).fill(0x90); // high bit set -> DER pads with 0x00
    const der = new Uint8Array([0x30, 0x45, 0x02, 0x20, ...r, 0x02, 0x21, 0x00, ...s]);
    const raw = normalizeEcdsaSignature(der);
    expect([...raw.slice(0, 32)]).toEqual([...r]);
    expect([...raw.slice(32)]).toEqual([...s]);
  });

  it('verifyPackageSignature reports a reason', async () => {
    const out = await verifyPackageSignature({ signature: 'x', certificate: 'y', content_hash: HASH, identity: 'i' }, env);
    expect(out.ok).toBe(false);
    expect(out.reason).toBeTruthy();
  });
});

describe('entry validation', () => {
  it.each([
    ['bad package name', { package_name: '../etc' }],
    ['bad version', { version: 'latest' }],
    ['bad hash', { content_hash: 'sha256:short' }],
    ['non-string field', { identity: 5 }],
    ['oversized field', { certificate: 'x'.repeat(20000) }],
  ])('rejects %s', async (_n, over) => {
    expect((await post(await goodBody(over))).status).toBe(400);
  });
  it('validateEntry rejects non-objects', () => {
    expect(validateEntry(null)).toBeTruthy();
  });
  it('rejects invalid JSON', async () => {
    const res = await handleRequest(
      new Request('https://log.test/api/v1/log/entries', { method: 'POST', headers: { 'X-API-Key': API_KEY }, body: '{' }),
      env,
    );
    expect(res.status).toBe(400);
  });
});

describe('append and query', () => {
  it('chains entries with increasing indices', async () => {
    await post(await goodBody());
    await post(await goodBody({ version: '1.0.1' }));
    const e1 = await (await get('/api/v1/log/entries/1')).json();
    const e0 = await (await get('/api/v1/log/entries/0')).json();
    expect(e1.prev_hash).toBe(e0.this_hash);
  });

  it('retries on an index collision instead of forking the chain', async () => {
    await post(await goodBody());
    // Simulate a racing writer: MAX(index) lags by one for the first read.
    const db = env.wares_transparency_log;
    const realPrepare = db.prepare.bind(db);
    let stale = true;
    db.prepare = (sql) => {
      if (stale && sql.includes('MAX("index")')) {
        stale = false;
        return { bind: () => ({ first: async () => ({ max_index: -1 }) }), first: async () => ({ max_index: -1 }) };
      }
      return realPrepare(sql);
    };
    const res = await post(await goodBody({ version: '2.0.0' }));
    expect(res.status).toBe(201);
    expect((await res.json()).index).toBe(1);
  });

  it('clamps limit and offset', async () => {
    await post(await goodBody());
    for (const q of ['limit=-1', 'limit=abc', 'offset=-5', 'limit=999999&offset=zzz']) {
      const res = await get(`/api/v1/log/query?${q}`);
      expect(res.status).toBe(200);
      const j = await res.json();
      expect(j.limit).toBeGreaterThanOrEqual(1);
      expect(j.limit).toBeLessThanOrEqual(1000);
      expect(j.offset).toBeGreaterThanOrEqual(0);
    }
  });

  it('intParam clamps', () => {
    expect(intParam('-1', 100, 1, 1000)).toBe(1);
    expect(intParam('abc', 100, 1, 1000)).toBe(100);
    expect(intParam(null, 100, 1, 1000)).toBe(100);
    expect(intParam('5000', 100, 1, 1000)).toBe(1000);
  });
});
