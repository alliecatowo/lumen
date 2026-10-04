import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync, writeFileSync } from 'node:fs';
import { handleRequest, parsePackagesPath, type Env } from '../worker';
import { FakeR2 } from './fake-r2';

/**
 * The `wares` CLI (rust/lumen-cli RegistryClient) reads a static registry layout.
 * These golden files are the contract: this test pins what the worker serves, and
 * rust/lumen-cli/tests/registry_contract.rs parses the very same files with the
 * CLI's own types. Regenerate with UPDATE_CONTRACT=1.
 */
const BASE = 'https://registry.test';
const dir = new URL('./contract/', import.meta.url);

let env: Env;

beforeEach(() => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: any, init?: any) => {
      if (String(input).endsWith('/applications/c/token')) {
        const tok = JSON.parse(init.body).access_token;
        return tok === 'tok'
          ? new Response(JSON.stringify({ user: { login: 'alice' } }), { status: 200 })
          : new Response('{}', { status: 404 });
      }
      throw new Error('unexpected fetch ' + input);
    }),
  );
  env = { REGISTRY_BUCKET: new FakeR2() as any, GITHUB_CLIENT_ID: 'c', GITHUB_CLIENT_SECRET: 's' };
});
afterEach(() => vi.unstubAllGlobals());

const call = (path: string, init: RequestInit = {}) => handleRequest(new Request(BASE + path, init), env);

async function publish(version: string, deps: Record<string, string>, body: Record<string, unknown> = {}) {
  const bytes = new TextEncoder().encode(`tarball ${version}`);
  const digest = Buffer.from(await crypto.subtle.digest('SHA-256', bytes)).toString('hex');
  return call('/v1/wares', {
    method: 'PUT',
    headers: { Authorization: 'Bearer tok', 'Content-Type': 'application/json' },
    body: JSON.stringify({
      name: '@t/dep',
      version,
      tarball: Buffer.from(bytes).toString('base64'),
      shasum: digest,
      description: 'A test package',
      deps,
      ...body,
    }),
  });
}

/** Replace volatile fields so documents can be compared with the golden files. */
function normalise(doc: any): any {
  const out = JSON.parse(JSON.stringify(doc));
  for (const key of ['published_at', 'updated_at']) if (out[key]) out[key] = '<timestamp>';
  for (const a of out.artifacts ?? []) a.hash = a.hash.replace(/[0-9a-f]{64}/, '<sha256>');
  for (const p of out.packages ?? []) if (p.updated_at) p.updated_at = '<timestamp>';
  return out;
}

async function golden(name: string, actual: unknown) {
  const url = new URL(name, dir);
  if (process.env.UPDATE_CONTRACT) writeFileSync(url, JSON.stringify(actual, null, 2) + '\n');
  expect(actual).toEqual(JSON.parse(readFileSync(url, 'utf8')));
}

describe('static registry layout', () => {
  beforeEach(async () => {
    expect((await publish('1.0.0', { '@t/leaf': '^1.0.0' })).status).toBe(201);
    expect((await publish('1.1.0-rc.1', {})).status).toBe(201);
  });

  it('serves the package index the CLI parses', async () => {
    const res = await call('/api/v1/packages/@t/dep/index.json');
    expect(res.status).toBe(200);
    await golden('package-index.json', normalise(await res.json()));
  });

  it('serves version metadata with a verifiable artifact', async () => {
    const res = await call('/api/v1/packages/@t/dep/1.0.0.json');
    expect(res.status).toBe(200);
    const doc = await res.json();
    await golden('version-metadata.json', normalise(doc));

    // The artifact URL is relative to the registry base and resolves to the stored tarball.
    const art = (doc as any).artifacts[0];
    const dl = await call(`/api/v1/${art.url}`);
    expect(dl.status).toBe(200);
    const bytes = new Uint8Array(await dl.arrayBuffer());
    const digest = Buffer.from(await crypto.subtle.digest('SHA-256', bytes)).toString('hex');
    expect(art.hash).toBe(`sha256:${digest}`);
  });

  it('serves the global index', async () => {
    const res = await call('/api/v1/index.json');
    expect(res.status).toBe(200);
    await golden('global-index.json', normalise(await res.json()));
  });

  it('404s unknown packages and versions, and versions without a recorded hash', async () => {
    expect((await call('/api/v1/packages/@t/nope/index.json')).status).toBe(404);
    expect((await call('/api/v1/packages/@t/dep/9.9.9.json')).status).toBe(404);
    await (env.REGISTRY_BUCKET as any).put(
      'wares/@t/legacy/index.json',
      JSON.stringify({ name: '@t/legacy', versions: ['0.1.0'], latest: '0.1.0', owner: null }),
    );
    expect((await call('/api/v1/packages/@t/legacy/0.1.0.json')).status).toBe(404);
  });

  it('rejects malformed deps on publish', async () => {
    for (const deps of [{ '../x': '^1' }, { '@t/a': '' }, ['@t/a'], { '@t/a': 5 }]) {
      const res = await publish('2.0.0', deps as any);
      expect(res.status, JSON.stringify(deps)).toBe(400);
    }
  });
});

describe('path parsing', () => {
  it('parses scoped and encoded names', () => {
    expect(parsePackagesPath('@t/dep/index.json')).toEqual({ name: '@t/dep', version: null });
    expect(parsePackagesPath('%40t%2Fdep/1.2.3.json')).toEqual({ name: '@t/dep', version: '1.2.3' });
    expect(parsePackagesPath('@t/dep/latest.json')).toBeNull();
    expect(parsePackagesPath('@t/dep')).toBeNull();
    expect(parsePackagesPath('../x/index.json')).toBeNull();
  });
});
