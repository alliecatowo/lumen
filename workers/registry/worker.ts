/**
 * Wares Registry Worker
 * 
 * Full-featured registry with:
 * - OIDC authentication (GitHub)
 * - Package publishing
 * - Transparency log integration
 * - R2 storage
 */

export interface Env {
  REGISTRY_BUCKET: R2Bucket;
  TRANSPARENCY_LOG_URL: string;
  TRANSPARENCY_LOG_API_KEY: string;
  GITHUB_CLIENT_ID?: string;
  GITHUB_CLIENT_SECRET?: string;
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

    try {
      // Health check
      if (path === '/health') {
        return json({ status: 'ok', service: 'wares-registry' }, corsHeaders);
      }

      // OIDC Authentication endpoints
      if (path === '/api/v1/auth/oidc/login' && method === 'POST') {
        return handleLogin(request, env, corsHeaders);
      }
      
      if (path.match(/^\/api\/v1\/auth\/oidc\/callback\/[^/]+$/) && method === 'GET') {
        const sessionId = path.split('/').pop()!;
        return handleCallback(sessionId, url, env, corsHeaders);
      }
      
      if (path.match(/^\/api\/v1\/auth\/oidc\/token\/[^/]+$/) && method === 'POST') {
        const sessionId = path.split('/').pop()!;
        return handleToken(sessionId, corsHeaders);
      }

      // Package endpoints
      if (path === '/v1/index' && method === 'GET') {
        return listPackages(env, corsHeaders);
      }

      if (path === '/v1/search' && method === 'GET') {
        return searchPackages(url, env, corsHeaders);
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
        hint: 'Use /health, /api/v1/auth/oidc/*, /v1/index, /v1/wares/*, /v1/search'
      }, corsHeaders, 404);

    } catch (e) {
      console.error('Error:', e);
      return json({ error: 'Internal error', details: String(e) }, corsHeaders, 500);
    }
  }
};

// OIDC Login
async function handleLogin(request: Request, env: Env, corsHeaders: Record<string, string>): Promise<Response> {
  const body = await request.json() as { provider: string };
  const provider = body.provider || 'github';
  
  const sessionId = generateId();
  const state = generateId();
  const pkceVerifier = generatePKCE();
  
  const baseUrl = getBaseUrl(request);
  const redirectUri = `${baseUrl}/api/v1/auth/oidc/callback/${sessionId}`;
  
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
  const authUrl = `https://github.com/login/oauth/authorize?` +
    `client_id=${clientId}&` +
    `redirect_uri=${encodeURIComponent(redirectUri)}&` +
    `state=${state}&` +
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
  const state = url.searchParams.get('state');
  const error = url.searchParams.get('error');
  
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

// Package management functions
async function listPackages(env: Env, corsHeaders: Record<string, string>): Promise<Response> {
  const list = await env.REGISTRY_BUCKET.list({ prefix: 'wares/' });
  const packages: string[] = [];
  
  for (const obj of list.objects) {
    if (obj.key?.endsWith('/index.json')) {
      const name = obj.key.replace('wares/', '').replace('/index.json', '');
      packages.push(name);
    }
  }
  
  return json(packages, corsHeaders);
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
  
  for (const obj of list.objects) {
    if (obj.key?.endsWith('/index.json')) {
      const name = obj.key.replace('wares/', '').replace('/index.json', '');
      if (!query || name.toLowerCase().includes(query.toLowerCase())) {
        const index = await env.REGISTRY_BUCKET.get(obj.key);
        if (index) {
          const data = await index.json() as any;
          results.push({ name, version: data.latest, description: data.description });
        }
      }
      if (results.length >= limit) break;
    }
  }
  
  return json({ packages: results, total: results.length }, corsHeaders);
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
  const body = await request.json() as any;
  const { name, version, tarball, shasum, signature } = body;
  
  if (!name || !version || !tarball) {
    return json({ error: 'Missing required fields' }, corsHeaders, 400);
  }

  // Upload tarball
  const tarballKey = `wares/${name}/${version}.tarball`;
  const tarballData = Uint8Array.from(atob(tarball), c => c.charCodeAt(0));
  await env.REGISTRY_BUCKET.put(tarballKey, tarballData, {
    httpMetadata: { contentType: 'application/gzip' },
  });

  // Submit to transparency log
  if (env.TRANSPARENCY_LOG_URL && env.TRANSPARENCY_LOG_API_KEY) {
    try {
      await fetch(`${env.TRANSPARENCY_LOG_URL}/api/v1/log/entries`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-API-Key': env.TRANSPARENCY_LOG_API_KEY
        },
        body: JSON.stringify({
          package_name: name,
          version,
          content_hash: `sha256:${shasum}`,
          identity: signature?.identity || 'unknown',
          signature: signature?.signature || '',
          certificate: signature?.certificate || ''
        })
      });
    } catch (e) {
      console.error('Transparency log error:', e);
    }
  }

  // Update index
  const indexKey = `wares/${name}/index.json`;
  let index: any = { name, versions: [], latest: null };
  
  const existing = await env.REGISTRY_BUCKET.get(indexKey);
  if (existing) {
    index = await existing.json();
  }
  
  if (!index.versions.includes(version)) {
    index.versions.push(version);
    index.versions.sort((a: string, b: string) => compareVersions(b, a));
    index.latest = index.versions[0];
  }
  
  await env.REGISTRY_BUCKET.put(indexKey, JSON.stringify(index), {
    httpMetadata: { contentType: 'application/json' },
  });

  return json({ success: true, name, version }, corsHeaders, 201);
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
