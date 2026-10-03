import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { handleRequest, parseWaresPath, type Env } from '../worker';
import { FakeR2 } from './fake-r2';

const CLIENT_ID = 'test-client';
const BASE = 'https://registry.test';

/** Tokens GitHub "issued to our app", token -> login. */
let appTokens: Record<string, string>;

function mockGithub() {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: any, init?: any) => {
      const url = String(input);
      if (url === `https://api.github.com/applications/${CLIENT_ID}/token`) {
        const tok = JSON.parse(init.body).access_token;
        const login = appTokens[tok];
        return login
          ? new Response(JSON.stringify({ user: { login, avatar_url: 'a' } }), { status: 200 })
          : new Response('{}', { status: 404 });
      }
      if (url === 'https://github.com/login/oauth/access_token') {
        return new Response(JSON.stringify({ access_token: 'gho_login_token' }), { status: 200 });
      }
      if (url === 'https://api.github.com/user') {
        return new Response(JSON.stringify({ login: 'alice' }), { status: 200 });
      }
      throw new Error(`unexpected fetch ${url}`);
    }),
  );
}

function b64(bytes: Uint8Array) {
  return Buffer.from(bytes).toString('base64');
}
async function sha256(bytes: Uint8Array) {
  const d = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes as any));
  return [...d].map((b) => b.toString(16).padStart(2, '0')).join('');
}

let bucket: FakeR2;
let logCalls: any[];
let logStatus: number;
let env: Env;

beforeEach(() => {
  appTokens = { 'tok-alice': 'alice', 'tok-bob': 'bob' };
  mockGithub();
  bucket = new FakeR2();
  logCalls = [];
  logStatus = 201;
  env = {
    REGISTRY_BUCKET: bucket as any,
    GITHUB_CLIENT_ID: CLIENT_ID,
    GITHUB_CLIENT_SECRET: 'secret',
    TRANSPARENCY_LOG_API_KEY: 'k',
    LOG_WORKER: {
      fetch: async (_u: string, init?: any) => {
        logCalls.push(JSON.parse(init.body));
        return new Response(JSON.stringify({ inserted: true, index: 7 }), { status: logStatus });
      },
    },
  };
});
afterEach(() => vi.unstubAllGlobals());

const call = (path: string, init: RequestInit = {}) => handleRequest(new Request(BASE + path, init), env);

async function publish(token: string | null, over: Record<string, unknown> = {}) {
  const bytes = new TextEncoder().encode('tarball-bytes-' + Math.random());
  const body = {
    name: '@alice/pkg',
    version: '1.0.0',
    tarball: b64(bytes),
    shasum: await sha256(bytes),
    signature: { signature: 's', certificate: 'c' },
    ...over,
  };
  return {
    bytes,
    res: await call('/v1/wares', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
      body: JSON.stringify(body),
    }),
  };
}

describe('publish auth and ownership', () => {
  it('rejects anonymous publishes and stores nothing', async () => {
    const { res } = await publish(null);
    expect(res.status).toBe(401);
    expect(bucket.store.size).toBe(0);
  });

  it('rejects a GitHub token that was not issued to this app', async () => {
    const { res } = await publish('some-other-apps-token');
    expect(res.status).toBe(401);
    expect(bucket.store.size).toBe(0);
  });

  it('fails closed when OAuth app credentials are not configured', async () => {
    delete env.GITHUB_CLIENT_SECRET;
    const { res } = await publish('tok-alice');
    expect(res.status).toBe(401);
  });

  it('records the owner and server-side hash on first publish', async () => {
    const { res, bytes } = await publish('tok-alice');
    expect(res.status).toBe(201);
    const idx = await (await call('/v1/wares/@alice/pkg')).json() as any;
    expect(idx.owner).toBe('github.com/alice');
    expect(idx.versionInfo['1.0.0'].shasum).toBe(await sha256(bytes));
    expect(logCalls[0].identity).toBe('github.com/alice');
    expect(logCalls[0].content_hash).toBe(`sha256:${await sha256(bytes)}`);
  });

  it("refuses another user's overwrite", async () => {
    await publish('tok-alice');
    const { res } = await publish('tok-bob', { version: '2.0.0' });
    expect(res.status).toBe(403);
    const idx = await (await call('/v1/wares/@alice/pkg')).json() as any;
    expect(idx.versions).toEqual(['1.0.0']);
    expect(bucket.store.has('wares/@alice/pkg/2.0.0.tarball')).toBe(false);
  });

  it('treats published versions as immutable, even for the owner', async () => {
    const first = await publish('tok-alice');
    const { res } = await publish('tok-alice');
    expect(res.status).toBe(409);
    const dl = await call('/v1/wares/@alice/pkg/1.0.0');
    expect(new Uint8Array(await dl.arrayBuffer())).toEqual(first.bytes);
  });

  it('does not let anyone publish into an ownerless legacy package', async () => {
    await bucket.put('wares/@alice/pkg/index.json', JSON.stringify({ name: '@alice/pkg', versions: ['0.1.0'], owner: null }));
    const { res } = await publish('tok-alice', { version: '0.2.0' });
    expect(res.status).toBe(403);
  });
});

describe('publish validation', () => {
  it.each([
    ['unscoped name', { name: 'pkg' }],
    ['traversal name', { name: '@a/b/../../x' }],
    ['uppercase name', { name: '@Alice/Pkg' }],
    ['bad semver', { version: '1.0' }],
    ['path in version', { version: '../1.0.0' }],
    ['bad base64', { tarball: '***' }],
    ['shasum mismatch', { shasum: 'ab'.repeat(32) }],
    ['huge description', { description: 'x'.repeat(5000) }],
    ['non-object proof', { proof: 'nope' }],
  ])('rejects %s', async (_n, over) => {
    const { res } = await publish('tok-alice', over);
    expect(res.status).toBe(400);
    expect(bucket.store.size).toBe(0);
  });

  it('treats a null proof like an absent one (the CLI sends null)', async () => {
    const { res } = await publish('tok-alice', { proof: null });
    expect(res.status).toBe(201);
  });

  it('rejects oversized tarballs', async () => {
    const big = new Uint8Array(10 * 1024 * 1024 + 1);
    const { res } = await publish('tok-alice', { tarball: b64(big), shasum: undefined });
    expect(res.status).toBe(413);
  });

  it('ignores a client-supplied author', async () => {
    await publish('tok-alice', { author: 'github.com/admin' });
    const idx = await (await call('/v1/wares/@alice/pkg')).json() as any;
    expect(idx.author).toBe('alice');
  });

  it('keeps latest at the highest stable release', async () => {
    await publish('tok-alice', { version: '1.0.0' });
    await publish('tok-alice', { version: '1.1.0-rc.1' });
    const idx = await (await call('/v1/wares/@alice/pkg')).json() as any;
    expect(idx.latest).toBe('1.0.0');
    expect(idx.versions).toEqual(['1.1.0-rc.1', '1.0.0']);
  });
});

describe('transparency log', () => {
  it('rejects the publish and stores nothing when the log refuses the entry', async () => {
    logStatus = 400;
    const { res } = await publish('tok-alice');
    expect(res.status).toBe(502);
    expect(bucket.store.size).toBe(0);
  });

  it('can be made advisory, and the version is marked unlogged', async () => {
    logStatus = 400;
    env.TRANSPARENCY_LOG_REQUIRED = 'false';
    const { res } = await publish('tok-alice');
    expect(res.status).toBe(201);
    const idx = await (await call('/v1/wares/@alice/pkg')).json() as any;
    expect(idx.versionInfo['1.0.0'].logged).toBe(false);
  });
});

describe('scoped routes and listing', () => {
  it('parses scoped and unscoped paths', () => {
    expect(parseWaresPath('@ns/name')).toEqual({ kind: 'package', name: '@ns/name' });
    expect(parseWaresPath('%40ns%2Fname/1.2.3')).toEqual({ kind: 'download', name: '@ns/name', version: '1.2.3' });
    expect(parseWaresPath('@ns/name/audit')).toEqual({ kind: 'audit', name: '@ns/name' });
    expect(parseWaresPath('@ns/name/1.0.0/resolve-proof')).toEqual({ kind: 'proof', name: '@ns/name', version: '1.0.0' });
    expect(parseWaresPath('@ns')).toBeNull();
    expect(parseWaresPath('..%2Fx')).toBeNull();
    expect(parseWaresPath('@ns/name/not-a-version')).toBeNull();
  });

  it('serves a scoped download', async () => {
    const { bytes } = await publish('tok-alice');
    const dl = await call('/api/v1/wares/@alice/pkg/1.0.0');
    expect(dl.status).toBe(200);
    expect(new Uint8Array(await dl.arrayBuffer())).toEqual(bytes);
  });

  it('lists packages beyond one R2 page', async () => {
    bucket.pageSize = 2;
    for (let i = 0; i < 5; i++) {
      await bucket.put(`wares/@a/p${i}/index.json`, JSON.stringify({ latest: '1.0.0' }));
      await bucket.put(`wares/@a/p${i}/1.0.0.tarball`, 'x');
    }
    const res = (await (await call('/v1/index')).json()) as any;
    expect(res.totalPackages).toBe(5);
  });

  it('clamps the search limit', async () => {
    const res = await call('/v1/search?limit=abc&q=');
    expect(res.status).toBe(200);
  });
});

describe('oauth login flow', () => {
  const verifier = 'v'.repeat(43);
  const login = async (extra: Record<string, unknown> = {}) =>
    call('/v1/auth/oidc/login', {
      method: 'POST',
      // Hard-coded SHA-256/base64url of `verifier`, as computed by the Rust CLI
      // (rust/lumen-cli/src/wares/trust.rs login_challenge_matches_the_worker_algorithm).
      body: JSON.stringify({ client_challenge: '7w_YNF9DSfIdPf_pRjSq646_kPr-2-o9NAl16JGghdM', ...extra }),
    });

  it('requires a client challenge and a safe redirect_uri', async () => {
    expect((await call('/v1/auth/oidc/login', { method: 'POST', body: '{}' })).status).toBe(400);
    expect((await login({ redirect_uri: 'https://evil.example/cb' })).status).toBe(400);
    expect((await login({ redirect_uri: 'http://127.0.0.1:8123/cb' })).status).toBe(200);
  });

  async function startAndCallback() {
    const started = (await (await login()).json()) as any;
    const state = new URL(started.auth_url).searchParams.get('state')!;
    const cb = await call(`/v1/auth/oidc/callback?code=c&state=${encodeURIComponent(state)}`);
    return { started, cb, state };
  }

  it('hands the token over once, only to the CLI, only after the code is confirmed', async () => {
    const { started, cb, state } = await startAndCallback();
    expect(cb.status).toBe(200);
    const tokenUrl = `/v1/auth/oidc/token?session_id=${started.session_id}`;
    const hdr = { 'X-Client-Verifier': verifier };

    // Not confirmed yet.
    expect((await call(tokenUrl, { headers: hdr })).status).toBe(202);
    // A caller who only knows the session id cannot poll.
    expect((await call(tokenUrl)).status).toBe(403);
    expect((await call(tokenUrl, { headers: { 'X-Client-Verifier': 'x'.repeat(43) } })).status).toBe(403);

    const confirm = await call('/v1/auth/oidc/confirm', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ session_id: started.session_id, user_code: started.user_code }),
    });
    expect(confirm.status).toBe(200);

    const tok = await call(tokenUrl, { headers: hdr });
    expect(tok.status).toBe(200);
    expect(((await tok.json()) as any).identity).toBe('github.com/alice');
    // One shot.
    expect((await call(tokenUrl, { headers: hdr })).status).toBe(404);
    // Callback cannot be replayed into a fresh session either.
    expect((await call(`/v1/auth/oidc/callback?code=c&state=${encodeURIComponent(state)}`)).status).toBe(404);
  });

  it('burns the session on a wrong confirmation code', async () => {
    const { started } = await startAndCallback();
    const bad = await call('/v1/auth/oidc/confirm', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ session_id: started.session_id, user_code: 'AAAA-AAAA' }),
    });
    expect(bad.status).toBe(403);
    const tok = await call(`/v1/auth/oidc/token?session_id=${started.session_id}`, { headers: { 'X-Client-Verifier': verifier } });
    expect(tok.status).toBe(400);
  });

  it('expires sessions', async () => {
    const started = (await (await login()).json()) as any;
    const key = `sessions/${started.session_id}.json`;
    const s = JSON.parse(await (await bucket.get(key))!.text());
    s.createdAt = Date.now() - 11 * 60 * 1000;
    await bucket.put(key, JSON.stringify(s));
    const tok = await call(`/v1/auth/oidc/token?session_id=${started.session_id}`, { headers: { 'X-Client-Verifier': verifier } });
    expect(tok.status).toBe(404);
  });
});

describe('certificates', () => {
  async function caKeyPem() {
    const kp = (await crypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, true, ['sign', 'verify'])) as CryptoKeyPair;
    const pk8 = new Uint8Array(await crypto.subtle.exportKey('pkcs8', kp.privateKey));
    return `-----BEGIN PRIVATE KEY-----\n${b64(pk8)}\n-----END PRIVATE KEY-----`;
  }
  async function userSpki() {
    const kp = (await crypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, true, ['sign', 'verify'])) as CryptoKeyPair;
    return b64(new Uint8Array(await crypto.subtle.exportKey('spki', kp.publicKey)));
  }
  const post = (body: unknown) => call('/v1/auth/cert', { method: 'POST', body: JSON.stringify(body) });

  it('refuses tokens from other OAuth apps', async () => {
    env.CA_PRIVATE_KEY = await caKeyPem();
    expect((await post({ oidc_token: 'foreign', public_key: await userSpki() })).status).toBe(401);
  });

  it('refuses malformed public keys', async () => {
    env.CA_PRIVATE_KEY = await caKeyPem();
    expect((await post({ oidc_token: 'tok-alice', public_key: b64(new Uint8Array(32)) })).status).toBe(400);
  });

  it('issues a certificate bound to the verified identity', async () => {
    env.CA_PRIVATE_KEY = await caKeyPem();
    const res = await post({ oidc_token: 'tok-alice', public_key: await userSpki() });
    expect(res.status).toBe(200);
    const cert = (await res.json()) as any;
    const certJson = JSON.parse(atob(cert.certificate_pem.split('\n')[1]));
    expect(certJson.subject).toBe('github.com/alice');
  });
});
