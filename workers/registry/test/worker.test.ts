import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { handleRequest, parseWaresPath, type Env } from '../worker';
import { FakeR2 } from './fake-r2';
import { makeArchive, makePackage } from './archive';
import { gzipSync } from 'node:zlib';

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
    ALLOWED_PUBLISHERS: 'alice,bob',
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
  const name = (over.name as string) ?? '@alice/pkg';
  const version = (over.version as string) ?? '1.0.0';
  const bytes = (over.bytes as Uint8Array) ?? makePackage(name, version);
  delete over.bytes;
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

  describe('callback is safe to load more than once', () => {
    const cbUrl = (state: string) => `/v1/auth/oidc/callback?code=c&state=${encodeURIComponent(state)}`;
    const exchanges = () =>
      (fetch as any).mock.calls.filter((c: any[]) => String(c[0]) === 'https://github.com/login/oauth/access_token').length;

    it('re-renders the confirm form while awaiting_confirmation, without re-exchanging the code', async () => {
      const { cb, state, started } = await startAndCallback();
      expect(cb.status).toBe(200);
      expect(exchanges()).toBe(1);
      const again = await call(cbUrl(state));
      expect(again.status).toBe(200);
      expect(again.headers.get('Content-Type')).toContain('text/html');
      const html = await again.text();
      expect(html).toContain('Confirm login');
      expect(html).toContain(started.session_id);
      expect(html).not.toContain('gho_login_token');
      expect(exchanges()).toBe(1);
      // A wrong state does not get the form.
      const bad = await call(cbUrl(`${started.session_id}:wrong`));
      expect(bad.status).toBe(400);
      expect(await bad.text()).not.toContain('Confirm login');
    });

    it('shows a readable failure page for a failed session', async () => {
      const { started, state } = await startAndCallback();
      await call('/v1/auth/oidc/confirm', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session_id: started.session_id, user_code: 'AAAA-AAAA' }),
      });
      const res = await call(cbUrl(state));
      expect(res.status).toBe(400);
      expect(res.headers.get('Content-Type')).toContain('text/html');
      const html = await res.text();
      expect(html).toContain('confirmation code was incorrect');
      expect(html).toContain('wares login');
      expect(exchanges()).toBe(1);
    });

    it('shows the success page for a completed session', async () => {
      const { started, state } = await startAndCallback();
      await call('/v1/auth/oidc/confirm', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session_id: started.session_id, user_code: started.user_code }),
      });
      const res = await call(cbUrl(state));
      expect(res.status).toBe(200);
      expect(await res.text()).toContain('Authentication successful');
      expect(exchanges()).toBe(1);
    });

    it('stores a failure reason and shows an HTML page when the token exchange fails', async () => {
      const started = (await (await login()).json()) as any;
      const state = new URL(started.auth_url).searchParams.get('state')!;
      const orig = globalThis.fetch;
      vi.stubGlobal('fetch', vi.fn(async (input: any, init?: any) => {
        if (String(input) === 'https://github.com/login/oauth/access_token') {
          return new Response(JSON.stringify({ error: 'bad_verification_code' }), { status: 200 });
        }
        return (orig as any)(input, init);
      }));
      const res = await call(cbUrl(state));
      expect(res.status).toBe(400);
      expect(res.headers.get('Content-Type')).toContain('text/html');
      const html = await res.text();
      expect(html).toContain('bad_verification_code');
      expect(html).toContain('wares login');
      // Reload shows the same page and the CLI sees a failure.
      const again = await call(cbUrl(state));
      expect(again.status).toBe(400);
      expect(await again.text()).toContain('bad_verification_code');
      const tok = await call(`/v1/auth/oidc/token?session_id=${started.session_id}`, { headers: { 'X-Client-Verifier': verifier } });
      expect(tok.status).toBe(400);
    });

    it('returns HTML for unknown sessions and bogus links', async () => {
      const res = await call('/v1/auth/oidc/callback?state=bogus:bogus');
      expect(res.status).toBe(404);
      expect(res.headers.get('Content-Type')).toContain('text/html');
      expect(await res.text()).toContain('not found or expired');
    });
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


const TOML = '[package]\nname = "@alice/pkg"\nversion = "1.0.0"\n';
const SRC = { path: 'src/main.lm', content: 'cell main() -> Int\n  return 1\nend\n' };

describe('publisher allowlist', () => {
  it('rejects a valid package from a non-allowlisted account with a clear 403', async () => {
    appTokens['tok-mallory'] = 'mallory';
    const { res } = await publish('tok-mallory');
    expect(res.status).toBe(403);
    const body = (await res.json()) as any;
    expect(body.error).toMatch(/invite-only/i);
    expect(bucket.store.size).toBe(0);
  });

  it('defaults to just alliecatowo when ALLOWED_PUBLISHERS is unset', async () => {
    delete env.ALLOWED_PUBLISHERS;
    expect((await publish('tok-alice')).res.status).toBe(403);
    appTokens['tok-allie'] = 'alliecatowo';
    expect((await publish('tok-allie', { name: '@alliecatowo/pkg' })).res.status).toBe(201);
  });

  it('matches numeric GitHub ids too', async () => {
    env.ALLOWED_PUBLISHERS = '4242';
    (globalThis.fetch as any).mockImplementation(async (input: any, init?: any) => {
      if (String(input).includes('/token')) return new Response(JSON.stringify({ user: { login: 'zed', id: 4242 } }), { status: 200 });
      throw new Error('unexpected ' + input);
    });
    expect((await publish('any')).res.status).toBe(201);
  });
});

describe('upload validation', () => {
  const rejects = async (bytes: Uint8Array, why: RegExp) => {
    const { res } = await publish('tok-alice', { bytes });
    expect(res.status).toBe(422);
    expect(((await res.json()) as any).error).toMatch(why);
    expect(bucket.store.size).toBe(0);
  };

  it('accepts a real package', async () => {
    expect((await publish('tok-alice')).res.status).toBe(201);
  });

  it('rejects non-archives (junk, html, svg, zip, png)', async () => {
    await rejects(new TextEncoder().encode('<html><script>alert(1)</script></html>'), /gzip/i);
    await rejects(new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg"><script/></svg>'), /gzip/i);
    await rejects(new Uint8Array([0x50, 0x4b, 3, 4, ...new Array(40).fill(0)]), /gzip/i);
    await rejects(new Uint8Array([0x89, 0x50, 0x4e, 0x47, ...new Array(40).fill(1)]), /gzip/i);
  });

  it('rejects a gzip that is not a tar', async () => {
    await rejects(new Uint8Array(gzipSync(Buffer.from('just some text, not a tar archive'.repeat(50)))), /tar|header|ustar/i);
  });

  it('rejects disallowed file types and binary content', async () => {
    await rejects(makeArchive([{ path: 'lumen.toml', content: TOML }, SRC, { path: 'src/evil.html', content: '<script>1</script>' }]), /not allowed/);
    await rejects(makeArchive([{ path: 'lumen.toml', content: TOML }, SRC, { path: 'src/x.svg', content: '<svg/>' }]), /not allowed/);
    await rejects(makeArchive([{ path: 'lumen.toml', content: TOML }, SRC, { path: 'src/run.exe', content: 'MZ' }]), /not allowed/);
    await rejects(makeArchive([{ path: 'lumen.toml', content: TOML }, { path: 'src/main.lm', content: new Uint8Array([0, 1, 2, 3]) }]), /Binary/);
  });

  it('rejects traversal, absolute paths, symlinks and hidden files', async () => {
    await rejects(makeArchive([{ path: 'lumen.toml', content: TOML }, SRC, { path: 'src/../../etc/x.lm', content: 'x' }]), /traversal|not allowed/);
    await rejects(makeArchive([{ path: '/abs/main.lm', content: 'x' }, { path: 'lumen.toml', content: TOML }]), /absolute|unsafe/);
    await rejects(makeArchive([{ path: 'lumen.toml', content: TOML }, SRC, { path: 'src/link.lm', type: '2' }]), /Entry type/);
    await rejects(makeArchive([{ path: 'lumen.toml', content: TOML }, SRC, { path: '.git/config', content: 'x' }]), /hidden|not allowed/);
  });

  it('requires a matching lumen.toml and at least one source file', async () => {
    await rejects(makeArchive([SRC]), /Missing lumen\.toml/);
    await rejects(makeArchive([{ path: 'lumen.toml', content: TOML }, { path: 'README.md', content: '# hi' }]), /no lumen source/);
    await rejects(makeArchive([{ path: 'lumen.toml', content: '[package]\nname = "other"\nversion = "1.0.0"\n' }, SRC]), /does not match/);
    await rejects(makeArchive([{ path: 'lumen.toml', content: '[package]\nname = "@alice/pkg"\nversion = "9.9.9"\n' }, SRC]), /version/);
  });

  it('stops decompression bombs', async () => {
    const big = 'a'.repeat(1_900_000);
    const entries = Array.from({ length: 40 }, (_, i) => ({ path: `src/f${i}.lm`, content: big }));
    await rejects(makeArchive([{ path: 'lumen.toml', content: TOML }, ...entries]), /decompression|expands/i);
  });

  it('caps the number of entries', async () => {
    const entries = Array.from({ length: 1100 }, (_, i) => ({ path: `src/f${i}.lm`, content: 'x' }));
    await rejects(makeArchive([{ path: 'lumen.toml', content: TOML }, ...entries]), /Too many entries/);
  });
});

describe('safe serving and yank', () => {
  it('serves downloads as an attachment with nosniff and a restrictive CSP', async () => {
    await publish('tok-alice');
    const dl = await call('/v1/wares/@alice/pkg/1.0.0');
    expect(dl.status).toBe(200);
    expect(dl.headers.get('Content-Type')).toBe('application/octet-stream');
    expect(dl.headers.get('Content-Disposition')).toMatch(/^attachment;/);
    expect(dl.headers.get('X-Content-Type-Options')).toBe('nosniff');
    expect(dl.headers.get('Content-Security-Policy')).toMatch(/default-src 'none'/);
  });

  it('lets the owner and admins yank, but nobody else', async () => {
    await publish('tok-alice');
    const del = (tok: string | null, path = '/v1/wares/@alice/pkg/1.0.0') =>
      call(path, { method: 'DELETE', headers: tok ? { Authorization: `Bearer ${tok}` } : {} });
    expect((await del(null)).status).toBe(401);
    expect((await del('tok-bob')).status).toBe(403);
    expect(bucket.store.has('wares/@alice/pkg/1.0.0.tarball')).toBe(true);
    expect((await del('tok-alice')).status).toBe(200);
    expect(bucket.store.has('wares/@alice/pkg/1.0.0.tarball')).toBe(false);
    expect((await call('/v1/wares/@alice/pkg/1.0.0')).status).toBe(404);

    await publish('tok-alice', { version: '2.0.0' });
    appTokens['tok-admin'] = 'root';
    env.ADMIN_USERS = 'root';
    expect((await del('tok-admin', '/v1/wares/@alice/pkg')).status).toBe(200);
    expect([...bucket.store.keys()].filter((k) => k.startsWith('wares/@alice/pkg'))).toEqual([]);
  });
});
