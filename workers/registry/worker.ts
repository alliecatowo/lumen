/**
 * Wares Registry Worker
 * 
 * Full-featured registry with:
 * - OIDC authentication (GitHub)
 * - Package publishing
 * - Transparency log integration
 * - Transparency log integration
 * - R2 storage
 */

import { CertificateAuthority, IdentityClaims } from './src/ca';

export interface Env {
  REGISTRY_BUCKET: R2Bucket;
  TRANSPARENCY_LOG_URL: string;
  TRANSPARENCY_LOG_API_KEY: string;
  GITHUB_CLIENT_ID?: string;
  GITHUB_CLIENT_SECRET?: string;
  CA_PRIVATE_KEY: string;
  CA_CERTIFICATE?: string;
}

// In-memory session storage (use KV in production)
const sessions = new Map<string, OAuthSession>();

interface OAuthSession {
  sessionId: string;
  provider: string;
  state: string;
  pkceVerifier: string;
  redirectUri: string;
  createdAt: number;
  status: 'pending' | 'completed' | 'failed';
  result?: OAuthResult;
}

interface OAuthResult {
  accessToken: string;
  identity: string;
  expiresIn: number;
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);
    let path = url.pathname;
    const method = request.method;

    // CORS headers
    const corsHeaders = {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
      'Access-Control-Allow-Headers': 'Authorization, Content-Type, X-API-Key',
    };

    // Handle CORS preflight
    if (method === 'OPTIONS') {
      return new Response(null, { headers: corsHeaders });
    }

    // Normalize path to handle both /v1 and /api/v1 prefixes
    if (path.startsWith('/api/v1')) {
      path = path.replace('/api/v1', '/v1');
    }

    try {
      // Health check (always available)
      if (path === '/health') {
        return json({ status: 'ok', service: 'wares-registry' }, corsHeaders);
      }

      // OIDC Authentication endpoints
      if (path === '/v1/auth/oidc/login' && method === 'POST') {
        return handleLogin(request, env, corsHeaders);
      }

      if (path === '/v1/auth/oidc/callback' && method === 'GET') {
        // Extract session ID from state parameter (format: sessionId:randomState)
        const stateParam = url.searchParams.get('state');
        if (!stateParam) {
          return json({ error: 'Missing state parameter' }, corsHeaders, 400);
        }
        const sessionId = stateParam.split(':')[0];
        return handleCallback(sessionId, url, env, corsHeaders);
      }

      if (path.match(/^\/v1\/auth\/oidc\/token/) && (method === 'POST' || method === 'GET')) {
        // Support both /token/:sessionId and /token?session_id=:sessionId
        let sessionId = path.split('/').pop()!;
        if (!sessionId || sessionId === 'token') {
          sessionId = url.searchParams.get('session_id') || '';
        }
        if (!sessionId) {
          return json({ error: 'Missing session_id' }, corsHeaders, 400);
        }
        return handleToken(sessionId, corsHeaders);
      }

      // Ephemeral certificate endpoint (Sigstore-style)
      if (path === '/v1/auth/cert' && method === 'POST') {
        return handleCert(request, env, corsHeaders);
      }

      // User profile
      if (path === '/v1/auth/user' && method === 'GET') {
        const user = await validateUser(request, env);
        if (!user) return json({ error: 'Unauthorized' }, corsHeaders, 401);

        // Fetch user's packages
        const list = await env.REGISTRY_BUCKET.list({ prefix: 'wares/' });
        const userPackages: any[] = [];
        for (const obj of list.objects) {
          if (obj.key?.endsWith('/index.json')) {
            const indexObj = await env.REGISTRY_BUCKET.get(obj.key);
            if (indexObj) {
              const data = await indexObj.json() as any;
              if (data.owner === user.identity) {
                userPackages.push(data);
              }
            }
          }
        }

        return json({ ...user, packages: userPackages }, corsHeaders);
      }

      // Package endpoints
      if (path === '/v1/index' && method === 'GET') {
        return listPackages(env, corsHeaders);
      }

      if (path === '/v1/search' && method === 'GET') {
        return searchPackages(url, env, corsHeaders);
      }

      // Package audit logs
      const auditMatch = path.match(/^\/v1\/wares\/([^/]+)\/audit$/);
      if (auditMatch && method === 'GET') {
        const name = auditMatch[1];
        if (env.TRANSPARENCY_LOG_URL || (env as any).LOG_WORKER) {
          try {
            const logBinding = (env as any).LOG_WORKER;
            const baseUrl = logBinding ? 'http://log.internal' : env.TRANSPARENCY_LOG_URL;
            console.log(`[DEBUG] Fetching audit for ${name} using ${logBinding ? 'Service Binding' : 'URL'}`);

            const [queryRes, logRes] = await Promise.all([
              (logBinding || { fetch }).fetch(`${baseUrl}/api/v1/log/query?package=${name}`),
              (logBinding || { fetch }).fetch(`${baseUrl}/api/v1/log`)
            ]);

            console.log(`[DEBUG] Query Status: ${queryRes.status}, Log Status: ${logRes.status}`);

            if (!queryRes.ok || !logRes.ok) {
              const errorText = await (!queryRes.ok ? queryRes.text() : logRes.text());
              console.error(`[DEBUG] Upstream error: ${errorText}`);
              throw new Error(`Upstream error: ${queryRes.status}/${logRes.status}`);
            }

            const queryData = await queryRes.json() as any;
            const logInfo = await logRes.json() as any;

            return json({
              entries: queryData.entries || [],
              total: queryData.total || 0,
              logInfo
            }, corsHeaders);
          } catch (e: any) {
            console.error('[DEBUG] Audit fetch error:', e.message);
            return json({ error: `Audit fetch failed: ${e.message}` }, corsHeaders, 500);
          }
        }
        return json({ error: 'Audit system unavailable' }, corsHeaders, 503);
      }

      // Resolution proof
      const proofMatch = path.match(/^\/v1\/wares\/([^/]+)\/resolve-proof$/);
      const versionedProofMatch = path.match(/^\/v1\/wares\/([^/]+)\/([^/]+)\/resolve-proof$/);

      if ((proofMatch || versionedProofMatch) && method === 'GET') {
        const name = proofMatch ? proofMatch[1] : versionedProofMatch![1];
        let version = versionedProofMatch ? versionedProofMatch[2] : null;

        if (!version) {
          const indexKey = `wares/${name}/index.json`;
          const indexObj = await env.REGISTRY_BUCKET.get(indexKey);
          if (indexObj) {
            const index = await indexObj.json() as any;
            version = index.latest;
          }
        }

        if (!version) {
          return json({ error: 'Package or version not found' }, corsHeaders, 404);
        }

        const proofKey = `wares/${name}/${version}.proof.json`;
        const proofObj = await env.REGISTRY_BUCKET.get(proofKey);

        if (!proofObj) {
          return json({
            error: 'Proof not found',
            hint: 'Proofs are generated during publication. Older packages may not have proofs.'
          }, corsHeaders, 404);
        }

        return new Response(proofObj.body, {
          headers: { 'Content-Type': 'application/json', ...corsHeaders }
        });
      }

      const waresMatch = path.match(/^\/v1\/wares\/([^/]+)$/);
      if (waresMatch && method === 'GET') {
        return getPackage(waresMatch[1], env, corsHeaders);
      }

      const downloadMatch = path.match(/^\/v1\/wares\/([^/]+)\/([^/]+)$/);
      if (downloadMatch && method === 'GET') {
        return downloadPackage(downloadMatch[1], downloadMatch[2], env, corsHeaders);
      }

      if (path === '/v1/wares' && method === 'PUT') {
        return publishPackage(request, env, corsHeaders);
      }

      return json({
        error: 'Not found',
        path,
        hint: 'Use /health, /v1/auth/oidc/*, /v1/index, /v1/wares/*, /v1/search'
      }, corsHeaders, 404);

    } catch (e) {
      console.error('Error:', e);
      return json({ error: 'Internal error', details: String(e) }, corsHeaders, 500);
    }
  }
};

// OIDC Login
async function handleLogin(request: Request, env: Env, corsHeaders: Record<string, string>): Promise<Response> {
  const body = await request.json() as { provider: string; redirect_uri?: string };
  const provider = body.provider || 'github';

  const sessionId = generateId();
  const state = generateId();
  const pkceVerifier = generatePKCE();

  const baseUrl = getBaseUrl(request);
  // Use client's redirect_uri if provided (for CLI localhost callback), otherwise use registry callback
  const redirectUri = body.redirect_uri || `${baseUrl}/api/v1/auth/oidc/callback`;

  const session: OAuthSession = {
    sessionId,
    provider,
    state,
    pkceVerifier,
    redirectUri,
    createdAt: Date.now(),
    status: 'pending'
  };

  sessions.set(sessionId, session);

  // Build GitHub OAuth URL
  const clientId = env.GITHUB_CLIENT_ID;
  if (!clientId) {
    return json({ error: 'GitHub OAuth not configured' }, corsHeaders, 500);
  }

  const pkceChallenge = await pkceChallengeFromVerifier(pkceVerifier);
  // Encode session_id in state parameter (format: sessionId:randomState)
  const stateParam = `${sessionId}:${state}`;
  const authUrl = `https://github.com/login/oauth/authorize?` +
    `client_id=${clientId}&` +
    `redirect_uri=${encodeURIComponent(redirectUri)}&` +
    `state=${stateParam}&` +
    `scope=read:user%20user:email&` +
    `response_type=code&` +
    `code_challenge=${pkceChallenge}&` +
    `code_challenge_method=S256`;

  return json({ session_id: sessionId, auth_url: authUrl }, corsHeaders);
}

// OAuth Callback
async function handleCallback(
  sessionId: string,
  url: URL,
  env: Env,
  corsHeaders: Record<string, string>
): Promise<Response> {
  const session = sessions.get(sessionId);
  if (!session) {
    return json({ error: 'Session not found' }, corsHeaders, 404);
  }

  const code = url.searchParams.get('code');
  const stateParam = url.searchParams.get('state');
  const error = url.searchParams.get('error');

  // Extract original random state from state parameter (format: sessionId:randomState)
  const state = stateParam?.split(':')[1];

  if (error) {
    session.status = 'failed';
    return json({ error: `OAuth error: ${error}` }, corsHeaders, 400);
  }

  if (!code || !state || state !== session.state) {
    return json({ error: 'Invalid code or state' }, corsHeaders, 400);
  }

  // Exchange code for token
  const tokenRes = await fetch('https://github.com/login/oauth/access_token', {
    method: 'POST',
    headers: {
      'Accept': 'application/json',
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      client_id: env.GITHUB_CLIENT_ID,
      client_secret: env.GITHUB_CLIENT_SECRET,
      code,
      redirect_uri: session.redirectUri,
      grant_type: 'authorization_code',
      code_verifier: session.pkceVerifier
    })
  });

  const tokenData = await tokenRes.json() as any;

  if (tokenData.error) {
    session.status = 'failed';
    return json({ error: tokenData.error_description || tokenData.error }, corsHeaders, 400);
  }

  // Fetch user info
  const userRes = await fetch('https://api.github.com/user', {
    headers: {
      'Authorization': `Bearer ${tokenData.access_token}`,
      'User-Agent': 'wares-registry/1.0'
    }
  });

  const userData = await userRes.json() as any;
  const identity = userData.login
    ? `github.com/${userData.login}`
    : `github.com/user/${userData.id}`;

  session.result = {
    accessToken: tokenData.access_token,
    identity,
    expiresIn: tokenData.expires_in || 3600
  };
  session.status = 'completed';

  // Return HTML for browser
  return new Response(`
    <html>
      <body style="font-family: sans-serif; max-width: 600px; margin: 50px auto; text-align: center;">
        <h1 style="color: #22c55e;">✓ Authentication Successful</h1>
        <p>Identity: <code>${identity}</code></p>
        <p>You can close this window and return to the CLI.</p>
      </body>
    </html>
  `, {
    headers: { 'Content-Type': 'text/html', ...corsHeaders }
  });
}

// Get Token
async function handleToken(
  sessionId: string,
  corsHeaders: Record<string, string>
): Promise<Response> {
  const session = sessions.get(sessionId);
  if (!session) {
    return json({ error: 'Session not found' }, corsHeaders, 404);
  }

  if (session.status === 'pending') {
    return json({ error: 'Authentication pending' }, corsHeaders, 202);
  }

  if (session.status === 'failed') {
    return json({ error: 'Authentication failed' }, corsHeaders, 400);
  }

  if (!session.result) {
    return json({ error: 'No result found' }, corsHeaders, 500);
  }

  return json({
    access_token: session.result.accessToken,
    identity: session.result.identity,
    expires_in: session.result.expiresIn
  }, corsHeaders);
}

// Ephemeral Certificate issuance (Sigstore-style)
async function handleCert(
  request: Request,
  env: Env,
  corsHeaders: Record<string, string>
): Promise<Response> {
  const body = await request.json() as { oidc_token: string; public_key: string };

  if (!body.oidc_token || !body.public_key) {
    return json({ error: 'Missing oidc_token or public_key' }, corsHeaders, 400);
  }

  // Verify the OIDC token with GitHub
  const userRes = await fetch('https://api.github.com/user', {
    headers: {
      'Authorization': `Bearer ${body.oidc_token}`,
      'User-Agent': 'wares-registry/1.0'
    }
  });

  if (!userRes.ok) {
    return json({ error: 'Invalid OIDC token' }, corsHeaders, 401);
  }

  const userData = await userRes.json() as any;
  const identity = userData.login
    ? `github.com/${userData.login}`
    : `github.com/user/${userData.id}`;

  try {
    if (!env.CA_PRIVATE_KEY) {
      console.error('CA_PRIVATE_KEY not configured');
      return json({ error: 'Server misconfiguration: CA key missing' }, corsHeaders, 500);
    }

    const ca = new CertificateAuthority(env.CA_PRIVATE_KEY);

    const nowSec = Math.floor(Date.now() / 1000);
    const identityClaims: IdentityClaims = {
      sub: identity,
      iss: 'https://github.com',
      aud: 'wares.lumen-lang.com',
      iat: nowSec,
      exp: nowSec + 600, // 10 minutes
      name: userData.name || userData.login,
    };

    const cert = await ca.issueCertificate(
      body.public_key,
      identityClaims
    );

    return json(cert, corsHeaders);

  } catch (e) {
    console.error('Certificate issuance failed:', e);
    return json({ error: 'Certificate issuance failed', details: String(e) }, corsHeaders, 500);
  }
}

// Package management functions
async function listPackages(env: Env, corsHeaders: Record<string, string>): Promise<Response> {
  const list = await env.REGISTRY_BUCKET.list({ prefix: 'wares/' });
  const packages: any[] = [];
  const seenNames = new Set<string>();

  for (const obj of list.objects) {
    if (obj.key?.endsWith('/index.json')) {
      const name = obj.key.replace('wares/', '').replace('/index.json', '');

      if (seenNames.has(name)) continue;
      seenNames.add(name);

      const indexObj = await env.REGISTRY_BUCKET.get(obj.key);
      if (indexObj) {
        const data = await indexObj.json() as any;
        packages.push({
          name,
          version: data.latest || '0.1.0',
          description: data.description || 'A Lumen package.',
          author: data.author || 'Anonymous',
          downloads: data.downloads || 0,
          keywords: data.keywords || [],
          isVerified: data.isVerified || false,
          owner: data.owner || null,
          updatedAt: data.updatedAt || new Date().toISOString()
        });
      }
    }
  }

  // Frontend expects this structure in index.vue
  return json({
    packages: packages.sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()),
    totalPackages: seenNames.size,
    totalDownloads: packages.reduce((acc, p) => acc + (p.downloads || 0), 0),
    categories: ['CLI', 'Utils', 'AI', 'HTTP', 'Database', 'Logic'],
    contributors: Array.from(new Set(packages.map(p => p.author))).length
  }, corsHeaders);
}

async function searchPackages(
  url: URL,
  env: Env,
  corsHeaders: Record<string, string>
): Promise<Response> {
  const query = url.searchParams.get('q') || '';
  const limit = parseInt(url.searchParams.get('limit') || '20');

  const list = await env.REGISTRY_BUCKET.list({ prefix: 'wares/' });
  const results: any[] = [];
  const seenNames = new Set<string>();

  for (const obj of list.objects) {
    if (obj.key?.endsWith('/index.json')) {
      const name = obj.key.replace('wares/', '').replace('/index.json', '');

      if (seenNames.has(name)) continue;

      if (!query || name.toLowerCase().includes(query.toLowerCase())) {
        const index = await env.REGISTRY_BUCKET.get(obj.key);
        if (index) {
          const data = await index.json() as any;
          seenNames.add(name);
          results.push({
            name,
            version: data.latest || '0.1.0',
            description: data.description || 'A Lumen package.',
            author: data.author || 'Anonymous',
            downloads: data.downloads || 0,
            keywords: data.keywords || [],
            isVerified: data.isVerified || false,
            owner: data.owner || null,
            updatedAt: data.updatedAt || new Date().toISOString()
          });
        }
      }
      if (results.length >= limit) break;
    }
  }

  // search.vue line 120: results.value = res?.results || []
  return json({ results: results, total: results.length }, corsHeaders);
}

async function getPackage(
  name: string,
  env: Env,
  corsHeaders: Record<string, string>
): Promise<Response> {
  const indexKey = `wares/${name}/index.json`;
  const index = await env.REGISTRY_BUCKET.get(indexKey);

  if (!index) {
    return json({ error: 'Package not found' }, corsHeaders, 404);
  }

  return new Response(index.body, {
    headers: { 'Content-Type': 'application/json', ...corsHeaders }
  });
}

async function downloadPackage(
  name: string,
  version: string,
  env: Env,
  corsHeaders: Record<string, string>
): Promise<Response> {
  const tarballKey = `wares/${name}/${version}.tarball`;
  const tarball = await env.REGISTRY_BUCKET.get(tarballKey);

  if (!tarball) {
    return json({ error: 'Version not found' }, corsHeaders, 404);
  }

  return new Response(tarball.body, {
    headers: {
      'Content-Type': 'application/gzip',
      'Content-Disposition': `attachment; filename="${name}-${version}.tgz"`,
      ...corsHeaders
    }
  });
}

async function publishPackage(
  request: Request,
  env: Env,
  corsHeaders: Record<string, string>
): Promise<Response> {
  const user = await validateUser(request, env);
  const body = await request.json() as any;
  const { name, version, tarball, shasum, signature, description, author } = body;

  if (!name || !version || !tarball) {
    return json({ error: 'Missing required fields' }, corsHeaders, 400);
  }

  // Check ownership
  const indexKey = `wares/${name}/index.json`;
  const existing = await env.REGISTRY_BUCKET.get(indexKey);
  let index: any = { name, versions: [], latest: null, owner: user?.identity || null };

  if (existing) {
    index = await existing.json();
    if (index.owner && user && index.owner !== user.identity) {
      return json({ error: 'Package owned by another user' }, corsHeaders, 403);
    }
  }

  // Update index metadata
  index.description = description || index.description;
  index.author = author || (user ? user.identity.split('/').pop() : 'Anonymous');
  if (user) {
    index.authorAvatar = user.avatar;
    index.authorIdentity = user.identity;
  }
  index.isVerified = !!user;
  index.updatedAt = new Date().toISOString();

  // Upload tarball
  const tarballKey = `wares/${name}/${version}.tarball`;
  const tarballData = Uint8Array.from(atob(tarball), c => c.charCodeAt(0));
  await env.REGISTRY_BUCKET.put(tarballKey, tarballData, {
    httpMetadata: { contentType: 'application/gzip' },
  });

  // Save resolution proof if provided (Phase 2 hardening)
  if (body.proof) {
    const proofKey = `wares/${name}/${version}.proof.json`;
    await env.REGISTRY_BUCKET.put(proofKey, JSON.stringify(body.proof), {
      httpMetadata: { contentType: 'application/json' },
    });
  }

  // Submit to transparency log
  if (env.TRANSPARENCY_LOG_URL || (env as any).LOG_WORKER) {
    try {
      const logBinding = (env as any).LOG_WORKER;
      const baseUrl = logBinding ? 'http://log.internal' : env.TRANSPARENCY_LOG_URL;

      await (logBinding || { fetch }).fetch(`${baseUrl}/api/v1/log/entries`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-API-Key': env.TRANSPARENCY_LOG_API_KEY
        },
        body: JSON.stringify({
          package_name: name,
          version,
          content_hash: `sha256:${shasum}`,
          identity: user?.identity || signature?.identity || 'unknown',
          signature: signature?.signature || 'none',
          certificate: signature?.certificate || 'none'
        })
      });
    } catch (e) {
      console.error('Transparency log error:', e);
    }
  }

  // Finalize index
  if (!index.versions.includes(version)) {
    index.versions.push(version);
  }

  // Ensure uniqueness and correct sorting
  index.versions = [...new Set(index.versions)].sort((a: any, b: any) => compareVersions(b, a));
  index.latest = index.versions[0];

  await env.REGISTRY_BUCKET.put(indexKey, JSON.stringify(index), {
    httpMetadata: { contentType: 'application/json' },
  });

  return json({ success: true, name, version }, corsHeaders, 201);
}

// User Validation
async function validateUser(request: Request, env: Env): Promise<{ identity: string; name?: string; avatar?: string } | null> {
  const auth = request.headers.get('Authorization');
  if (!auth || !auth.startsWith('Bearer ')) return null;
  const token = auth.split(' ')[1];

  // In a real system, we'd verify the token with GitHub
  // For this implementation, we fetch user info from GitHub to validate
  try {
    const res = await fetch('https://api.github.com/user', {
      headers: {
        'Authorization': `Bearer ${token}`,
        'User-Agent': 'wares-registry/1.0'
      }
    });

    if (!res.ok) return null;
    const data = await res.json() as any;
    return {
      identity: `github.com/${data.login}`,
      name: data.name || data.login,
      avatar: data.avatar_url
    };
  } catch (e) {
    return null;
  }
}

// Helpers
function json(data: any, headers: Record<string, string>, status = 200): Response {
  return new Response(JSON.stringify(data, null, 2), {
    status,
    headers: { 'Content-Type': 'application/json', ...headers }
  });
}

function generateId(): string {
  return crypto.randomUUID();
}

function generatePKCE(): string {
  const array = new Uint8Array(32);
  crypto.getRandomValues(array);
  return btoa(String.fromCharCode(...array))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=/g, '');
}

async function pkceChallengeFromVerifier(verifier: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(verifier);
  const digest = await crypto.subtle.digest('SHA-256', data);
  return btoa(String.fromCharCode(...new Uint8Array(digest)))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=/g, '');
}

function getBaseUrl(request: Request): string {
  const url = new URL(request.url);
  return `${url.protocol}//${url.host}`;
}

function compareVersions(a: string, b: string): number {
  const partsA = a.split('.').map(Number);
  const partsB = b.split('.').map(Number);
  for (let i = 0; i < Math.max(partsA.length, partsB.length); i++) {
    const partA = partsA[i] || 0;
    const partB = partsB[i] || 0;
    if (partA !== partB) return partA - partB;
  }
  return 0;
}
