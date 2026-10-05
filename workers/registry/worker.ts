/**
 * Wares Registry Worker
 *
 * - GitHub OAuth login (device-style confirmation, one-shot token hand-off)
 * - Authenticated, ownership-checked, immutable package publishing
 * - Ephemeral signing certificates
 * - Transparency log integration
 * - R2 storage
 *
 * Auth model: every credential is a GitHub OAuth access token that was issued
 * to THIS OAuth app (verified via the applications/{client_id}/token API).
 */

import { CertificateAuthority, IdentityClaims } from './src/ca';
import { compareVersions, parseSemver, pickLatest } from './src/semver';
import { validatePackageArchive } from './src/package';
import {
  MAX_DEPS,
  MAX_DESCRIPTION_LEN,
  MAX_PROOF_BYTES,
  MAX_PUBLISH_BODY_BYTES,
  MAX_TARBALL_BYTES,
  decodeBase64,
  isValidPublishName,
  isValidReadName,
  sha256Hex,
  timingSafeEqual,
} from './src/validate';

export interface Env {
  REGISTRY_BUCKET: R2Bucket;
  TRANSPARENCY_LOG_URL?: string;
  TRANSPARENCY_LOG_API_KEY?: string;
  /** Set to "false" to let publishes succeed (marked unlogged) when the log rejects them. */
  TRANSPARENCY_LOG_REQUIRED?: string;
  GITHUB_CLIENT_ID?: string;
  GITHUB_CLIENT_SECRET?: string;
  CA_PRIVATE_KEY?: string;
  CA_CERTIFICATE?: string;
  /**
   * Comma-separated GitHub logins and/or numeric ids allowed to publish. Publishing is
   * invite-only until moderation exists. Unset means just "alliecatowo".
   */
  ALLOWED_PUBLISHERS?: string;
  /** Comma-separated logins/ids that may yank any package. Unset means "alliecatowo". */
  ADMIN_USERS?: string;
  LOG_WORKER?: { fetch: (input: string, init?: RequestInit) => Promise<Response> };
}

type Headers_ = Record<string, string>;

const SESSION_TTL_MS = 10 * 60 * 1000;
const USER_CODE_ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';

interface OAuthSession {
  sessionId: string;
  provider: string;
  state: string;
  pkceVerifier: string;
  redirectUri: string;
  /** base64url(sha256(client_verifier)); the CLI proves knowledge at /token. */
  clientChallenge: string;
  /** Short code the CLI shows; the browser user must type it to confirm. */
  userCode: string;
  createdAt: number;
  status: 'pending' | 'awaiting_confirmation' | 'completed' | 'failed';
  result?: OAuthResult;
  /** Human-readable reason shown in the browser when status is 'failed'. Never contains secrets. */
  failureReason?: string;
}

interface OAuthResult {
  accessToken: string;
  identity: string;
  expiresIn: number;
}

interface User {
  identity: string;
  login?: string;
  id?: number;
  name?: string;
  avatar?: string;
}

export default {
  async fetch(request: Request, env: Env, _ctx?: ExecutionContext): Promise<Response> {
    return handleRequest(request, env);
  },
};

export async function handleRequest(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  let path = url.pathname;
  const method = request.method;

  const corsHeaders: Headers_ = {
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
    'X-Content-Type-Options': 'nosniff',
    'Access-Control-Allow-Headers': 'Authorization, Content-Type, X-Client-Verifier',
  };

  if (method === 'OPTIONS') {
    return new Response(null, { headers: corsHeaders });
  }

  // Normalize path to handle both /v1 and /api/v1 prefixes
  if (path.startsWith('/api/v1')) {
    path = path.replace('/api/v1', '/v1');
  }

  try {
    if (path === '/health') {
      return json({ status: 'ok', service: 'wares-registry' }, corsHeaders);
    }

    // ---- OAuth endpoints -------------------------------------------------
    if (path === '/v1/auth/oidc/login' && method === 'POST') {
      return handleLogin(request, env, corsHeaders);
    }

    if (path === '/v1/auth/oidc/callback' && method === 'GET') {
      const stateParam = url.searchParams.get('state');
      if (!stateParam) {
        return errorPage('Invalid login link', 'Run `wares login` again in your terminal to start over.', corsHeaders, 400);
      }
      return handleCallback(stateParam.split(':')[0], url, env, corsHeaders);
    }

    if (path === '/v1/auth/oidc/confirm' && method === 'POST') {
      return handleConfirm(request, env, corsHeaders);
    }

    if (/^\/v1\/auth\/oidc\/token/.test(path) && (method === 'POST' || method === 'GET')) {
      let sessionId = path.split('/').pop() ?? '';
      if (!sessionId || sessionId === 'token') {
        sessionId = url.searchParams.get('session_id') || '';
      }
      if (!sessionId) {
        return json({ error: 'Missing session_id' }, corsHeaders, 400);
      }
      let verifier = request.headers.get('X-Client-Verifier') || '';
      if (!verifier && method === 'POST') {
        try {
          verifier = ((await request.json()) as any)?.client_verifier || '';
        } catch {
          /* no body */
        }
      }
      return handleToken(sessionId, verifier, env, corsHeaders);
    }

    if (path === '/v1/auth/cert' && method === 'POST') {
      return handleCert(request, env, corsHeaders);
    }

    if (path === '/v1/auth/user' && method === 'GET') {
      const user = await validateUser(request, env);
      if (!user) return json({ error: 'Unauthorized' }, corsHeaders, 401);

      const userPackages: any[] = [];
      for (const key of await listIndexKeys(env)) {
        const indexObj = await env.REGISTRY_BUCKET.get(key);
        if (indexObj) {
          const data = (await indexObj.json()) as any;
          if (data.owner === user.identity) userPackages.push(data);
        }
      }
      return json({ ...user, packages: userPackages }, corsHeaders);
    }

    // ---- Package endpoints ----------------------------------------------
    if (path === '/v1/index' && method === 'GET') {
      return listPackages(env, corsHeaders);
    }

    // Static registry layout read by the `wares` CLI (RegistryClient):
    //   GET /index.json, /packages/<@ns/name>/index.json, /packages/<@ns/name>/<version>.json
    if (path === '/v1/index.json' && method === 'GET') {
      return globalIndex(url, env, corsHeaders);
    }
    if (path.startsWith('/v1/packages/') && method === 'GET') {
      const route = parsePackagesPath(path.slice('/v1/packages/'.length));
      if (!route) return json({ error: 'Invalid package path' }, corsHeaders, 400);
      return route.version === null
        ? packageIndexDoc(route.name, env, corsHeaders)
        : versionMetadataDoc(route.name, route.version, env, corsHeaders);
    }

    if (path === '/v1/search' && method === 'GET') {
      return searchPackages(url, env, corsHeaders);
    }

    if (path === '/v1/wares' && method === 'PUT') {
      return publishPackage(request, env, corsHeaders);
    }

    if (path.startsWith('/v1/wares/') && method === 'DELETE') {
      return yankPackage(request, path.slice('/v1/wares/'.length), env, corsHeaders);
    }

    if (path.startsWith('/v1/wares/') && method === 'GET') {
      const route = parseWaresPath(path.slice('/v1/wares/'.length));
      if (!route) return json({ error: 'Invalid package path' }, corsHeaders, 400);
      switch (route.kind) {
        case 'package':
          return getPackage(route.name, env, corsHeaders);
        case 'download':
          return downloadPackage(route.name, route.version, env, corsHeaders);
        case 'audit':
          return auditPackage(route.name, env, corsHeaders);
        case 'proof':
          return resolveProof(route.name, route.version, env, corsHeaders);
      }
    }

    return json(
      { error: 'Not found', path, hint: 'Use /health, /v1/auth/oidc/*, /v1/index, /v1/wares/*, /v1/search' },
      corsHeaders,
      404,
    );
  } catch (e) {
    console.error('Error:', e);
    return json({ error: 'Internal error' }, corsHeaders, 500);
  }
}

// ---------------------------------------------------------------------------
// Routing helpers
// ---------------------------------------------------------------------------

export type WaresRoute =
  | { kind: 'package'; name: string }
  | { kind: 'download'; name: string; version: string }
  | { kind: 'audit'; name: string }
  | { kind: 'proof'; name: string; version: string | null };

/**
 * Parse the part of the path after `/v1/wares/`. Scoped names (`@ns/name`)
 * span two segments; `%2F`-encoded slashes are also accepted.
 */
export function parseWaresPath(rest: string): WaresRoute | null {
  let segs: string[];
  try {
    segs = rest.split('/').filter((s) => s.length > 0).map(decodeURIComponent);
  } catch {
    return null;
  }
  // Re-split segments that contained an encoded slash (e.g. "@ns%2Fname").
  segs = segs.flatMap((s) => (s.startsWith('@') && s.includes('/') ? s.split('/') : [s]));
  if (segs.length === 0) return null;

  let name: string;
  let tail: string[];
  if (segs[0].startsWith('@')) {
    if (segs.length < 2) return null;
    name = `${segs[0]}/${segs[1]}`;
    tail = segs.slice(2);
  } else {
    name = segs[0];
    tail = segs.slice(1);
  }
  if (!isValidReadName(name)) return null;

  if (tail.length === 0) return { kind: 'package', name };
  if (tail.length === 1) {
    if (tail[0] === 'audit') return { kind: 'audit', name };
    if (tail[0] === 'resolve-proof') return { kind: 'proof', name, version: null };
    return parseSemver(tail[0]) ? { kind: 'download', name, version: tail[0] } : null;
  }
  if (tail.length === 2 && tail[1] === 'resolve-proof' && parseSemver(tail[0])) {
    return { kind: 'proof', name, version: tail[0] };
  }
  return null;
}

/** Parse `<name>/index.json` or `<name>/<version>.json` below `/v1/packages/`. */
export function parsePackagesPath(rest: string): { name: string; version: string | null } | null {
  let segs: string[];
  try {
    segs = rest.split('/').filter((x) => x.length > 0).map(decodeURIComponent);
  } catch {
    return null;
  }
  segs = segs.flatMap((x) => (x.startsWith('@') && x.includes('/') ? x.split('/') : [x]));
  if (segs.length < 2) return null;
  const name = segs[0].startsWith('@') ? `${segs[0]}/${segs[1]}` : segs[0];
  const tail = segs.slice(segs[0].startsWith('@') ? 2 : 1);
  if (!isValidReadName(name) || tail.length !== 1 || !tail[0].endsWith('.json')) return null;
  const doc = tail[0].slice(0, -'.json'.length);
  if (doc === 'index') return { name, version: null };
  return parseSemver(doc) ? { name, version: doc } : null;
}

// ---------------------------------------------------------------------------
// OAuth session storage (R2, strongly consistent, shared across isolates)
// ---------------------------------------------------------------------------

const sessionKey = (id: string) => `sessions/${id}.json`;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

async function loadSession(env: Env, id: string): Promise<OAuthSession | null> {
  if (!UUID_RE.test(id)) return null;
  const obj = await env.REGISTRY_BUCKET.get(sessionKey(id));
  if (!obj) return null;
  const session = (await obj.json()) as OAuthSession;
  if (Date.now() - session.createdAt > SESSION_TTL_MS) {
    await env.REGISTRY_BUCKET.delete(sessionKey(id));
    return null;
  }
  return session;
}

async function saveSession(env: Env, session: OAuthSession): Promise<void> {
  await env.REGISTRY_BUCKET.put(sessionKey(session.sessionId), JSON.stringify(session), {
    httpMetadata: { contentType: 'application/json' },
  });
}

function isAllowedRedirect(uri: string, baseUrl: string): boolean {
  if (uri === `${baseUrl}/api/v1/auth/oidc/callback`) return true;
  try {
    const u = new URL(uri);
    return u.protocol === 'http:' && ['127.0.0.1', 'localhost', '[::1]'].includes(u.hostname);
  } catch {
    return false;
  }
}

function generateUserCode(): string {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  const chars = Array.from(bytes, (b) => USER_CODE_ALPHABET[b % USER_CODE_ALPHABET.length]);
  return `${chars.slice(0, 4).join('')}-${chars.slice(4).join('')}`;
}

// ---------------------------------------------------------------------------
// OAuth login flow
// ---------------------------------------------------------------------------

async function handleLogin(request: Request, env: Env, corsHeaders: Headers_): Promise<Response> {
  let body: { provider?: string; redirect_uri?: string; client_challenge?: string };
  try {
    body = (await request.json()) as any;
  } catch {
    return json({ error: 'Invalid JSON body' }, corsHeaders, 400);
  }
  const provider = body.provider || 'github';
  if (provider !== 'github') {
    return json({ error: 'Unsupported provider' }, corsHeaders, 400);
  }
  const clientId = env.GITHUB_CLIENT_ID;
  if (!clientId || !env.GITHUB_CLIENT_SECRET) {
    return json({ error: 'GitHub OAuth not configured' }, corsHeaders, 500);
  }
  if (!body.client_challenge || !/^[A-Za-z0-9_-]{43}$/.test(body.client_challenge)) {
    return json(
      { error: 'client_challenge (base64url SHA-256 of a CLI-held secret) is required' },
      corsHeaders,
      400,
    );
  }

  const baseUrl = getBaseUrl(request);
  const redirectUri = body.redirect_uri || `${baseUrl}/api/v1/auth/oidc/callback`;
  if (!isAllowedRedirect(redirectUri, baseUrl)) {
    return json({ error: 'redirect_uri must be the registry callback or a loopback address' }, corsHeaders, 400);
  }

  const session: OAuthSession = {
    sessionId: generateId(),
    provider,
    state: generateId(),
    pkceVerifier: generatePKCE(),
    redirectUri,
    clientChallenge: body.client_challenge,
    userCode: generateUserCode(),
    createdAt: Date.now(),
    status: 'pending',
  };
  await saveSession(env, session);

  const pkceChallenge = await pkceChallengeFromVerifier(session.pkceVerifier);
  const authUrl =
    `https://github.com/login/oauth/authorize?` +
    `client_id=${encodeURIComponent(clientId)}&` +
    `redirect_uri=${encodeURIComponent(redirectUri)}&` +
    `state=${encodeURIComponent(`${session.sessionId}:${session.state}`)}&` +
    `scope=read:user%20user:email&` +
    `response_type=code&` +
    `code_challenge=${pkceChallenge}&` +
    `code_challenge_method=S256`;

  return json(
    { session_id: session.sessionId, auth_url: authUrl, user_code: session.userCode, expires_in: SESSION_TTL_MS / 1000 },
    corsHeaders,
  );
}

function htmlPage(body: string, corsHeaders: Headers_, status = 200): Response {
  return new Response(
    `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Wares login</title></head>` +
      `<body style="font-family: sans-serif; max-width: 600px; margin: 50px auto; padding: 0 16px; text-align: center;">${body}</body></html>`,
    { status, headers: { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store', ...corsHeaders } },
  );
}

function errorPage(title: string, detail: string, corsHeaders: Headers_, status: number): Response {
  return htmlPage(`<h1>${escapeHtml(title)}</h1><p>${escapeHtml(detail)}</p>`, corsHeaders, status);
}

function confirmPage(session: OAuthSession, corsHeaders: Headers_): Response {
  const identity = session.result?.identity ?? '';
  return htmlPage(
    `<h1>Confirm login</h1>
      <p>Signed in as <code>${escapeHtml(identity)}</code>.</p>
      <p>Type the code shown in your terminal. If you did not start a login with <code>wares login</code>, close this tab.</p>
      <form method="POST" action="/api/v1/auth/oidc/confirm">
        <input type="hidden" name="session_id" value="${escapeHtml(session.sessionId)}">
        <input name="user_code" autocomplete="off" autofocus placeholder="XXXX-XXXX" style="font-size: 1.4em; text-align: center; width: 90%;">
        <button type="submit" style="font-size: 1.4em; margin-top: 12px;">Confirm</button>
      </form>`,
    corsHeaders,
  );
}

function successPage(corsHeaders: Headers_): Response {
  return htmlPage('<h1>Authentication successful</h1><p>You can close this window and return to the CLI.</p>', corsHeaders);
}

function failedPage(session: OAuthSession, corsHeaders: Headers_): Response {
  return errorPage(
    'Login failed',
    `${session.failureReason ?? 'This login attempt failed.'} Run \`wares login\` again in your terminal to start over.`,
    corsHeaders,
    400,
  );
}

async function failSession(env: Env, session: OAuthSession, reason: string): Promise<void> {
  console.log(`[oidc] ${session.sessionId.slice(0, 8)} ${session.status} -> failed: ${reason}`);
  session.status = 'failed';
  session.failureReason = reason;
  session.result = undefined;
  await saveSession(env, session);
}

/** Render the page matching a session that is not (or no longer) pending. Safe to hit repeatedly. */
function pageForSettledSession(session: OAuthSession, corsHeaders: Headers_): Response {
  switch (session.status) {
    case 'awaiting_confirmation':
      return confirmPage(session, corsHeaders);
    case 'completed':
      return successPage(corsHeaders);
    default:
      return failedPage(session, corsHeaders);
  }
}

async function handleCallback(sessionId: string, url: URL, env: Env, corsHeaders: Headers_): Promise<Response> {
  const sid = sessionId.slice(0, 8);
  const session = await loadSession(env, sessionId);
  if (!session) {
    console.log(`[oidc] ${sid} callback: session not found or expired`);
    return errorPage(
      'Login session not found or expired',
      'Run `wares login` again in your terminal to start over.',
      corsHeaders,
      404,
    );
  }

  const code = url.searchParams.get('code');
  const error = url.searchParams.get('error');
  const state = url.searchParams.get('state')?.split(':')[1];
  const stateOk = !!state && timingSafeEqual(state, session.state);

  // Reloads, prefetches and double navigations: never re-exchange the code.
  if (session.status !== 'pending') {
    console.log(`[oidc] ${sid} callback reloaded in status ${session.status}`);
    if (!stateOk) {
      return errorPage('Invalid login link', 'Run `wares login` again in your terminal to start over.', corsHeaders, 400);
    }
    return pageForSettledSession(session, corsHeaders);
  }

  if (error) {
    if (!stateOk) {
      return errorPage('Invalid login link', 'Run `wares login` again in your terminal to start over.', corsHeaders, 400);
    }
    await failSession(env, session, 'GitHub authorization was denied.');
    return failedPage(session, corsHeaders);
  }
  if (!code || !stateOk) {
    console.log(`[oidc] ${sid} callback: invalid code or state`);
    return errorPage('Invalid login link', 'Run `wares login` again in your terminal to start over.', corsHeaders, 400);
  }

  // A concurrent duplicate request may have settled the session while we were exchanging.
  const settledMeanwhile = async (): Promise<Response | null> => {
    const fresh = await loadSession(env, sessionId);
    if (fresh && fresh.status !== 'pending') {
      console.log(`[oidc] ${sid} concurrent callback already settled the session (${fresh.status})`);
      return pageForSettledSession(fresh, corsHeaders);
    }
    return null;
  };

  const tokenRes = await fetch('https://github.com/login/oauth/access_token', {
    method: 'POST',
    headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
    body: JSON.stringify({
      client_id: env.GITHUB_CLIENT_ID,
      client_secret: env.GITHUB_CLIENT_SECRET,
      code,
      redirect_uri: session.redirectUri,
      code_verifier: session.pkceVerifier,
    }),
  });
  const tokenData = (await tokenRes.json().catch(() => ({}))) as any;
  if (tokenData.error || !tokenData.access_token) {
    const settled = await settledMeanwhile();
    if (settled) return settled;
    await failSession(env, session, `GitHub rejected the token exchange (${String(tokenData.error ?? 'no access token')}).`);
    return failedPage(session, corsHeaders);
  }

  const userRes = await fetch('https://api.github.com/user', {
    headers: { Authorization: `Bearer ${tokenData.access_token}`, 'User-Agent': 'wares-registry/1.0' },
  });
  if (!userRes.ok) {
    const settled = await settledMeanwhile();
    if (settled) return settled;
    await failSession(env, session, `Could not read your GitHub profile (HTTP ${userRes.status}).`);
    return failedPage(session, corsHeaders);
  }
  const userData = (await userRes.json()) as any;
  const identity = identityFromGithub(userData);

  const settled = await settledMeanwhile();
  if (settled) return settled;

  session.result = {
    accessToken: tokenData.access_token,
    identity,
    expiresIn: tokenData.expires_in || 3600,
  };
  console.log(`[oidc] ${sid} pending -> awaiting_confirmation`);
  session.status = 'awaiting_confirmation';
  await saveSession(env, session);
  return confirmPage(session, corsHeaders);
}

async function handleConfirm(request: Request, env: Env, corsHeaders: Headers_): Promise<Response> {
  let sessionId = '';
  let userCode = '';
  const ctype = request.headers.get('Content-Type') || '';
  try {
    if (ctype.includes('application/json')) {
      const b = (await request.json()) as any;
      sessionId = b.session_id || '';
      userCode = b.user_code || '';
    } else {
      const form = await request.formData();
      sessionId = String(form.get('session_id') || '');
      userCode = String(form.get('user_code') || '');
    }
  } catch {
    return json({ error: 'Invalid request body' }, corsHeaders, 400);
  }
  const session = await loadSession(env, sessionId);
  if (!session || session.status !== 'awaiting_confirmation') {
    return json({ error: 'Session not found, expired, or not awaiting confirmation' }, corsHeaders, 404);
  }
  if (!timingSafeEqual(userCode.trim().toUpperCase(), session.userCode)) {
    // A wrong code burns the session: the code space is small.
    await failSession(env, session, 'The confirmation code was incorrect.');
    return json({ error: 'Incorrect code; start the login again' }, corsHeaders, 403);
  }
  console.log(`[oidc] ${session.sessionId.slice(0, 8)} awaiting_confirmation -> completed`);
  session.status = 'completed';
  await saveSession(env, session);
  return successPage(corsHeaders);
}

async function handleToken(
  sessionId: string,
  clientVerifier: string,
  env: Env,
  corsHeaders: Headers_,
): Promise<Response> {
  const session = await loadSession(env, sessionId);
  if (!session) {
    return json({ error: 'Session not found or expired' }, corsHeaders, 404);
  }
  // Prove the caller is the CLI that started the login.
  const challenge = clientVerifier ? await pkceChallengeFromVerifier(clientVerifier) : '';
  if (!challenge || !timingSafeEqual(challenge, session.clientChallenge)) {
    return json({ error: 'Invalid client_verifier' }, corsHeaders, 403);
  }
  if (session.status === 'pending' || session.status === 'awaiting_confirmation') {
    return json({ error: 'Authentication pending' }, corsHeaders, 202);
  }
  if (session.status === 'failed' || !session.result) {
    return json({ error: 'Authentication failed' }, corsHeaders, 400);
  }

  // One-shot: the token can be collected exactly once.
  await env.REGISTRY_BUCKET.delete(sessionKey(session.sessionId));
  return json(
    {
      access_token: session.result.accessToken,
      identity: session.result.identity,
      expires_in: session.result.expiresIn,
    },
    corsHeaders,
  );
}

// ---------------------------------------------------------------------------
// Ephemeral certificates
// ---------------------------------------------------------------------------

async function handleCert(request: Request, env: Env, corsHeaders: Headers_): Promise<Response> {
  let body: { oidc_token?: string; public_key?: string };
  try {
    body = (await request.json()) as any;
  } catch {
    return json({ error: 'Invalid JSON body' }, corsHeaders, 400);
  }
  if (!body.oidc_token || !body.public_key) {
    return json({ error: 'Missing oidc_token or public_key' }, corsHeaders, 400);
  }

  const user = await verifyAccessToken(body.oidc_token, env);
  if (!user) {
    return json({ error: 'Invalid token' }, corsHeaders, 401);
  }

  // The public key must be a P-256 SPKI key.
  const spki = decodeBase64(body.public_key);
  if (!spki || spki.length > 200) {
    return json({ error: 'public_key must be base64 SPKI' }, corsHeaders, 400);
  }
  try {
    await crypto.subtle.importKey('spki', spki as any, { name: 'ECDSA', namedCurve: 'P-256' }, false, ['verify']);
  } catch {
    return json({ error: 'public_key is not a valid P-256 SPKI key' }, corsHeaders, 400);
  }

  if (!env.CA_PRIVATE_KEY) {
    console.error('CA_PRIVATE_KEY not configured');
    return json({ error: 'Server misconfiguration: CA key missing' }, corsHeaders, 500);
  }
  try {
    const ca = new CertificateAuthority(env.CA_PRIVATE_KEY);
    const nowSec = Math.floor(Date.now() / 1000);
    const claims: IdentityClaims = {
      sub: user.identity,
      iss: 'https://github.com',
      aud: 'wares.lumen-lang.com',
      iat: nowSec,
      exp: nowSec + 600,
      name: user.name,
    };
    return json(await ca.issueCertificate(body.public_key, claims), corsHeaders);
  } catch (e) {
    console.error('Certificate issuance failed:', e);
    return json({ error: 'Certificate issuance failed' }, corsHeaders, 500);
  }
}

// ---------------------------------------------------------------------------
// Package reads
// ---------------------------------------------------------------------------

/** List every `wares/<name>/index.json` key, following R2 pagination cursors. */
async function listIndexKeys(env: Env): Promise<string[]> {
  const keys: string[] = [];
  let cursor: string | undefined;
  do {
    const page = await env.REGISTRY_BUCKET.list({ prefix: 'wares/', cursor });
    for (const obj of page.objects) {
      if (obj.key.endsWith('/index.json')) keys.push(obj.key);
    }
    cursor = page.truncated ? page.cursor : undefined;
  } while (cursor);
  return keys;
}

function summarize(name: string, data: any) {
  return {
    name,
    version: data.latest || '0.1.0',
    description: data.description || 'A Lumen package.',
    author: data.author || 'Anonymous',
    downloads: data.downloads || 0,
    keywords: data.keywords || [],
    isVerified: data.isVerified || false,
    owner: data.owner || null,
    updatedAt: data.updatedAt || new Date().toISOString(),
  };
}

const nameFromKey = (key: string) => key.slice('wares/'.length, -'/index.json'.length);

async function listPackages(env: Env, corsHeaders: Headers_): Promise<Response> {
  const packages: any[] = [];
  for (const key of await listIndexKeys(env)) {
    const indexObj = await env.REGISTRY_BUCKET.get(key);
    if (indexObj) packages.push(summarize(nameFromKey(key), await indexObj.json()));
  }
  return json(
    {
      packages: packages.sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()),
      totalPackages: packages.length,
      totalDownloads: packages.reduce((acc, p) => acc + (p.downloads || 0), 0),
      categories: ['CLI', 'Utils', 'AI', 'HTTP', 'Database', 'Logic'],
      contributors: new Set(packages.map((p) => p.author)).size,
    },
    corsHeaders,
  );
}

async function searchPackages(url: URL, env: Env, corsHeaders: Headers_): Promise<Response> {
  const query = (url.searchParams.get('q') || '').toLowerCase();
  const parsed = parseInt(url.searchParams.get('limit') || '20', 10);
  const limit = Number.isFinite(parsed) ? Math.min(Math.max(parsed, 1), 100) : 20;

  const results: any[] = [];
  for (const key of await listIndexKeys(env)) {
    const name = nameFromKey(key);
    if (query && !name.toLowerCase().includes(query)) continue;
    const index = await env.REGISTRY_BUCKET.get(key);
    if (index) results.push(summarize(name, await index.json()));
    if (results.length >= limit) break;
  }
  return json({ results, total: results.length }, corsHeaders);
}

async function getPackage(name: string, env: Env, corsHeaders: Headers_): Promise<Response> {
  const index = await env.REGISTRY_BUCKET.get(`wares/${name}/index.json`);
  if (!index) {
    return json({ error: 'Package not found' }, corsHeaders, 404);
  }
  return new Response(index.body, { headers: { 'Content-Type': 'application/json', ...corsHeaders } });
}

/** `GET /index.json`: the registry listing in the CLI's `GlobalIndex` shape. */
async function globalIndex(url: URL, env: Env, corsHeaders: Headers_): Promise<Response> {
  const packages: any[] = [];
  for (const key of await listIndexKeys(env)) {
    const obj = await env.REGISTRY_BUCKET.get(key);
    if (!obj) continue;
    const data = (await obj.json()) as any;
    packages.push({
      name: nameFromKey(key),
      latest: data.latest ?? null,
      description: data.description ?? null,
      updated_at: data.updatedAt ?? null,
    });
  }
  return json(
    {
      name: 'wares',
      version: '1',
      updated_at: new Date().toISOString(),
      package_count: packages.length,
      packages,
    },
    corsHeaders,
  );
}

/** `GET /packages/<name>/index.json`: the CLI's `RegistryPackageIndex`. */
async function packageIndexDoc(name: string, env: Env, corsHeaders: Headers_): Promise<Response> {
  const obj = await env.REGISTRY_BUCKET.get(`wares/${name}/index.json`);
  if (!obj) return json({ error: 'Package not found' }, corsHeaders, 404);
  const data = (await obj.json()) as any;
  const versions: string[] = data.versions || [];
  return json(
    {
      name,
      versions: [...versions].sort(compareVersions),
      latest: data.latest ?? null,
      yanked: data.yanked || {},
      prereleases: versions.filter((v) => (parseSemver(v)?.prerelease.length ?? 0) > 0),
      description: data.description ?? null,
    },
    corsHeaders,
  );
}

/** `GET /packages/<name>/<version>.json`: the CLI's `RegistryVersionMetadata`. */
async function versionMetadataDoc(name: string, version: string, env: Env, corsHeaders: Headers_): Promise<Response> {
  const obj = await env.REGISTRY_BUCKET.get(`wares/${name}/index.json`);
  if (!obj) return json({ error: 'Package not found' }, corsHeaders, 404);
  const data = (await obj.json()) as any;
  const info = data.versionInfo?.[version];
  // Versions published before the server recorded their hash cannot be verified, so
  // they are not offered to installers.
  if (!info?.shasum) return json({ error: 'Version not found' }, corsHeaders, 404);
  return json(
    {
      name,
      version,
      deps: info.deps || {},
      artifacts: [
        {
          kind: 'tgz',
          // Relative to the registry base URL (…/api/v1); served by downloadPackage.
          url: `wares/${name}/${version}`,
          hash: `sha256:${info.shasum}`,
          size: info.size ?? null,
        },
      ],
      yanked: Boolean(data.yanked?.[version]),
      published_at: info.publishedAt ?? null,
      description: data.description ?? null,
    },
    corsHeaders,
  );
}

async function downloadPackage(name: string, version: string, env: Env, corsHeaders: Headers_): Promise<Response> {
  const tarball = await env.REGISTRY_BUCKET.get(`wares/${name}/${version}.tarball`);
  if (!tarball) {
    return json({ error: 'Version not found' }, corsHeaders, 404);
  }
  return new Response(tarball.body, {
    headers: {
      // Uploaded bytes are never rendered by a browser on a lumen-lang.com origin.
      'Content-Type': 'application/octet-stream',
      'Content-Disposition': `attachment; filename="${name.replace('/', '-').replace('@', '')}-${version}.tgz"`,
      'Content-Security-Policy': "default-src 'none'; frame-ancestors 'none'; sandbox",
      'Cross-Origin-Resource-Policy': 'same-site',
      'Cache-Control': 'public, max-age=31536000, immutable',
      ...corsHeaders,
    },
  });
}

async function resolveProof(name: string, version: string | null, env: Env, corsHeaders: Headers_): Promise<Response> {
  if (!version) {
    const indexObj = await env.REGISTRY_BUCKET.get(`wares/${name}/index.json`);
    if (indexObj) version = ((await indexObj.json()) as any).latest ?? null;
  }
  if (!version) {
    return json({ error: 'Package or version not found' }, corsHeaders, 404);
  }
  const proofObj = await env.REGISTRY_BUCKET.get(`wares/${name}/${version}.proof.json`);
  if (!proofObj) {
    return json(
      { error: 'Proof not found', hint: 'Proofs are generated during publication. Older packages may not have proofs.' },
      corsHeaders,
      404,
    );
  }
  return new Response(proofObj.body, { headers: { 'Content-Type': 'application/json', ...corsHeaders } });
}

async function auditPackage(name: string, env: Env, corsHeaders: Headers_): Promise<Response> {
  const logBinding = env.LOG_WORKER;
  if (!logBinding && !env.TRANSPARENCY_LOG_URL) {
    return json({ error: 'Audit system unavailable' }, corsHeaders, 503);
  }
  try {
    const baseUrl = logBinding ? 'http://log.internal' : env.TRANSPARENCY_LOG_URL!;
    const f = (u: string) => (logBinding ? logBinding.fetch(u) : fetch(u));
    const [queryRes, logRes] = await Promise.all([
      f(`${baseUrl}/api/v1/log/query?package=${encodeURIComponent(name)}`),
      f(`${baseUrl}/api/v1/log`),
    ]);
    if (!queryRes.ok || !logRes.ok) {
      throw new Error(`Upstream error: ${queryRes.status}/${logRes.status}`);
    }
    const queryData = (await queryRes.json()) as any;
    const logInfo = await logRes.json();
    return json({ entries: queryData.entries || [], total: queryData.total || 0, logInfo }, corsHeaders);
  } catch (e) {
    console.error('Audit fetch error:', e);
    return json({ error: 'Audit fetch failed' }, corsHeaders, 502);
  }
}

// ---------------------------------------------------------------------------
// Publish
// ---------------------------------------------------------------------------

async function publishPackage(request: Request, env: Env, corsHeaders: Headers_): Promise<Response> {
  const user = await validateUser(request, env);
  if (!user) {
    return json({ error: 'Authentication required to publish' }, corsHeaders, 401);
  }
  if (!isListed(user, env.ALLOWED_PUBLISHERS, DEFAULT_PUBLISHERS)) {
    return json(
      {
        error: 'Publishing is invite-only for now',
        detail:
          'The Wares registry only accepts packages from approved publishers while moderation is being built. ' +
          'Open an issue at https://github.com/alliecatowo/lumen to request access.',
      },
      corsHeaders,
      403,
    );
  }

  const declared = parseInt(request.headers.get('Content-Length') || '0', 10);
  if (declared > MAX_PUBLISH_BODY_BYTES) {
    return json({ error: 'Request body too large' }, corsHeaders, 413);
  }
  const raw = await request.text();
  if (raw.length > MAX_PUBLISH_BODY_BYTES) {
    return json({ error: 'Request body too large' }, corsHeaders, 413);
  }
  let body: any;
  try {
    body = JSON.parse(raw);
  } catch {
    return json({ error: 'Invalid JSON body' }, corsHeaders, 400);
  }
  if (!body || typeof body !== 'object') {
    return json({ error: 'Invalid JSON body' }, corsHeaders, 400);
  }

  const { name, version, tarball, shasum, signature, description } = body;
  if (!name || !version || !tarball) {
    return json({ error: 'Missing required fields' }, corsHeaders, 400);
  }
  if (!isValidPublishName(name)) {
    return json({ error: 'Invalid package name; expected @namespace/name (lowercase, digits, dashes)' }, corsHeaders, 400);
  }
  if (!parseSemver(version)) {
    return json({ error: 'Invalid version; expected semver' }, corsHeaders, 400);
  }
  if (description !== undefined && (typeof description !== 'string' || description.length > MAX_DESCRIPTION_LEN)) {
    return json({ error: `description must be a string of at most ${MAX_DESCRIPTION_LEN} characters` }, corsHeaders, 400);
  }
  let proofJson: string | null = null;
  if (body.proof !== undefined && body.proof !== null) {
    proofJson = JSON.stringify(body.proof);
    if (typeof body.proof !== 'object' || body.proof === null || proofJson.length > MAX_PROOF_BYTES) {
      return json({ error: 'proof must be a JSON object under 64 KiB' }, corsHeaders, 400);
    }
  }

  let deps: Record<string, string> = {};
  if (body.deps !== undefined) {
    const d = body.deps;
    const entries = d && typeof d === 'object' && !Array.isArray(d) ? Object.entries(d) : null;
    if (
      !entries ||
      entries.length > MAX_DEPS ||
      !entries.every(
        ([k, v]) => isValidReadName(k) && typeof v === 'string' && v.length > 0 && v.length <= 64,
      )
    ) {
      return json({ error: 'deps must map valid package names to version constraints' }, corsHeaders, 400);
    }
    deps = Object.fromEntries(entries as [string, string][]);
  }

  const data = decodeBase64(tarball);
  if (!data) {
    return json({ error: 'tarball must be valid base64' }, corsHeaders, 400);
  }
  if (data.length > MAX_TARBALL_BYTES) {
    return json({ error: 'Tarball too large' }, corsHeaders, 413);
  }

  const archive = await validatePackageArchive(data, { name, version });
  if (!archive.ok) {
    return json(
      { error: `Not a valid lumen package: ${archive.error}`, hint: 'Upload the .tgz produced by `wares pack`' },
      corsHeaders,
      422,
    );
  }

  const digest = await sha256Hex(data);
  if (shasum !== undefined) {
    const claimed = String(shasum).replace(/^sha256:/i, '').toLowerCase();
    if (claimed !== digest) {
      return json({ error: 'shasum does not match the uploaded tarball' }, corsHeaders, 400);
    }
  }

  const indexKey = `wares/${name}/index.json`;
  const tarballKey = `wares/${name}/${version}.tarball`;

  // Authorization + immutability checks before any side effect.
  const existing = await env.REGISTRY_BUCKET.get(indexKey);
  if (existing) {
    const current = (await existing.json()) as any;
    if (!current.owner) {
      return json({ error: 'Package has no owner; contact the registry administrators to claim it' }, corsHeaders, 403);
    }
    if (current.owner !== user.identity) {
      return json({ error: 'Package owned by another user' }, corsHeaders, 403);
    }
    if ((current.versions || []).includes(version)) {
      return json({ error: `Version ${version} already exists and is immutable` }, corsHeaders, 409);
    }
  }

  // Transparency log: fail the publish if the log rejects it (unless opted out).
  let logIndex: number | null = null;
  const logBinding = env.LOG_WORKER;
  if (logBinding || env.TRANSPARENCY_LOG_URL) {
    const required = env.TRANSPARENCY_LOG_REQUIRED !== 'false';
    let failure: string | null = null;
    try {
      const baseUrl = logBinding ? 'http://log.internal' : env.TRANSPARENCY_LOG_URL!;
      const init: RequestInit = {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-API-Key': env.TRANSPARENCY_LOG_API_KEY || '' },
        body: JSON.stringify({
          package_name: name,
          version,
          content_hash: `sha256:${digest}`,
          identity: user.identity,
          signature: signature?.signature,
          certificate: signature?.certificate,
        }),
      };
      const target = `${baseUrl}/api/v1/log/entries`;
      const res = await (logBinding ? logBinding.fetch(target, init) : fetch(target, init));
      if (res.ok) {
        logIndex = ((await res.json()) as any)?.index ?? null;
      } else {
        failure = `transparency log responded ${res.status}`;
      }
    } catch (e) {
      console.error('Transparency log error:', e);
      failure = 'transparency log unreachable';
    }
    if (failure && required) {
      return json({ error: `Publish rejected: ${failure}`, hint: 'Packages must be signed and logged' }, corsHeaders, 502);
    }
  }

  // Store the tarball; R2 refuses to overwrite an existing key.
  const stored = await env.REGISTRY_BUCKET.put(tarballKey, data, {
    httpMetadata: { contentType: 'application/gzip' },
    onlyIf: { etagDoesNotMatch: '*' },
  });
  if (!stored) {
    return json({ error: `Version ${version} already exists and is immutable` }, corsHeaders, 409);
  }
  if (proofJson) {
    await env.REGISTRY_BUCKET.put(`wares/${name}/${version}.proof.json`, proofJson, {
      httpMetadata: { contentType: 'application/json' },
    });
  }

  // Update the index with compare-and-swap so concurrent publishes cannot drop versions.
  for (let attempt = 0; attempt < 5; attempt++) {
    const cur = await env.REGISTRY_BUCKET.get(indexKey);
    const index: any = cur
      ? await cur.json()
      : { name, versions: [], latest: null, owner: user.identity, versionInfo: {} };
    if (index.owner !== user.identity) {
      await env.REGISTRY_BUCKET.delete(tarballKey);
      return json({ error: 'Package owned by another user' }, corsHeaders, 403);
    }
    if (description) index.description = description;
    index.author = user.identity.split('/').pop();
    index.authorAvatar = user.avatar;
    index.authorIdentity = user.identity;
    index.isVerified = true;
    index.updatedAt = new Date().toISOString();
    index.versions = [...new Set([...(index.versions || []), version])].sort((a: string, b: string) =>
      compareVersions(b, a),
    );
    index.latest = pickLatest(index.versions);
    index.versionInfo = {
      ...(index.versionInfo || {}),
      [version]: {
        shasum: digest,
        size: data.length,
        deps,
        publishedAt: index.updatedAt,
        publisher: user.identity,
        logged: logIndex !== null,
        logIndex,
      },
    };
    const ok = await env.REGISTRY_BUCKET.put(indexKey, JSON.stringify(index), {
      httpMetadata: { contentType: 'application/json' },
      onlyIf: cur ? { etagMatches: cur.etag } : { etagDoesNotMatch: '*' },
    });
    if (ok) {
      return json({ success: true, name, version, shasum: digest, logIndex, logged: logIndex !== null }, corsHeaders, 201);
    }
  }
  await env.REGISTRY_BUCKET.delete(tarballKey);
  return json({ error: 'Concurrent publish conflict; retry' }, corsHeaders, 409);
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

function identityFromGithub(data: any): string {
  return data.login ? `github.com/${data.login}` : `github.com/user/${data.id}`;
}

/**
 * Verify `token` was issued to THIS OAuth app and return its user. Fails
 * closed when the OAuth app credentials are not configured.
 */
export async function verifyAccessToken(token: string, env: Env): Promise<User | null> {
  if (!token || !env.GITHUB_CLIENT_ID || !env.GITHUB_CLIENT_SECRET) return null;
  try {
    const res = await fetch(`https://api.github.com/applications/${encodeURIComponent(env.GITHUB_CLIENT_ID)}/token`, {
      method: 'POST',
      headers: {
        Authorization: `Basic ${btoa(`${env.GITHUB_CLIENT_ID}:${env.GITHUB_CLIENT_SECRET}`)}`,
        Accept: 'application/vnd.github+json',
        'Content-Type': 'application/json',
        'User-Agent': 'wares-registry/1.0',
      },
      body: JSON.stringify({ access_token: token }),
    });
    if (!res.ok) return null;
    const data = (await res.json()) as any;
    if (!data?.user?.login) return null;
    return {
      identity: identityFromGithub(data.user),
      login: data.user.login,
      id: data.user.id,
      name: data.user.name || data.user.login,
      avatar: data.user.avatar_url,
    };
  } catch {
    return null;
  }
}

const DEFAULT_PUBLISHERS = 'alliecatowo';

/** True if the user's GitHub login or numeric id is in the comma-separated list. */
export function isListed(user: User, list: string | undefined, fallback: string): boolean {
  const entries = (list === undefined || list.trim() === '' ? fallback : list)
    .split(',')
    .map((s) => s.trim().toLowerCase())
    .filter(Boolean);
  const login = (user.login ?? user.identity.split('/').pop() ?? '').toLowerCase();
  return entries.includes(login) || (user.id !== undefined && entries.includes(String(user.id)));
}

/**
 * Remove a version (or the whole package with no version) from the registry. Allowed for the
 * package owner and for ADMIN_USERS: the fast path to pull a bad package.
 * DELETE /v1/wares/<name>/<version>   or   DELETE /v1/wares/<name>
 */
async function yankPackage(request: Request, rest: string, env: Env, corsHeaders: Headers_): Promise<Response> {
  const user = await validateUser(request, env);
  if (!user) return json({ error: 'Authentication required' }, corsHeaders, 401);
  const route = parseWaresPath(rest);
  if (!route || (route.kind !== 'download' && route.kind !== 'package')) {
    return json({ error: 'Invalid package path' }, corsHeaders, 400);
  }
  const indexKey = `wares/${route.name}/index.json`;
  const obj = await env.REGISTRY_BUCKET.get(indexKey);
  const index: any = obj ? await obj.json() : null;
  const admin = isListed(user, env.ADMIN_USERS, DEFAULT_PUBLISHERS);
  if (!admin && !(index && index.owner === user.identity)) {
    return json({ error: 'Only the package owner or a registry admin can yank' }, corsHeaders, 403);
  }
  if (!index) return json({ error: 'Package not found' }, corsHeaders, 404);

  const versions: string[] = route.kind === 'download' ? [route.version] : [...(index.versions || [])];
  for (const v of versions) {
    await env.REGISTRY_BUCKET.delete(`wares/${route.name}/${v}.tarball`);
    await env.REGISTRY_BUCKET.delete(`wares/${route.name}/${v}.proof.json`);
  }
  if (route.kind === 'package') {
    await env.REGISTRY_BUCKET.delete(indexKey);
    return json({ success: true, removed: versions, package: route.name }, corsHeaders);
  }
  index.versions = (index.versions || []).filter((x: string) => x !== route.version);
  index.latest = pickLatest(index.versions);
  index.yanked = { ...(index.yanked || {}), [route.version]: { by: user.identity, at: new Date().toISOString() } };
  if (index.versionInfo) delete index.versionInfo[route.version];
  await env.REGISTRY_BUCKET.put(indexKey, JSON.stringify(index), { httpMetadata: { contentType: 'application/json' } });
  return json({ success: true, removed: versions, package: route.name }, corsHeaders);
}

async function validateUser(request: Request, env: Env): Promise<User | null> {
  const auth = request.headers.get('Authorization');
  if (!auth || !auth.startsWith('Bearer ')) return null;
  return verifyAccessToken(auth.slice('Bearer '.length).trim(), env);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function json(data: any, headers: Headers_, status = 200): Response {
  return new Response(JSON.stringify(data, null, 2), {
    status,
    headers: { 'Content-Type': 'application/json', ...headers },
  });
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
}

function generateId(): string {
  return crypto.randomUUID();
}

function b64url(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
}

function generatePKCE(): string {
  const array = new Uint8Array(32);
  crypto.getRandomValues(array);
  return b64url(array);
}

async function pkceChallengeFromVerifier(verifier: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier));
  return b64url(new Uint8Array(digest));
}

function getBaseUrl(request: Request): string {
  const url = new URL(request.url);
  return `${url.protocol}//${url.host}`;
}
