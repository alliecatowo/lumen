export default {
  async fetch(request, env, ctx): Promise<Response> {
    const url = new URL(request.url);
    let path = url.pathname;
    
    // Strip /api prefix if present (for custom domain routes)
    if (path.startsWith('/api')) {
      path = path.slice(4);
    }
    
    const method = request.method;
    
    // CORS headers
    const corsHeaders = {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
      'Access-Control-Allow-Headers': 'Authorization, Content-Type',
    };

    // Handle CORS preflight
    if (method === 'OPTIONS') {
      return new Response(null, { headers: corsHeaders });
    }

    // Simple auth check for write operations
    const isWrite = method === 'PUT' || method === 'DELETE';
    const authHeader = request.headers.get('Authorization');
    
    if (isWrite) {
      // For now, require any auth header - we'll improve this
      if (!authHeader) {
        return new Response(JSON.stringify({ error: 'Authentication required' }), {
          status: 401,
          headers: { 'Content-Type': 'application/json', ...corsHeaders },
        });
      }
    }

    try {
      // GET /v1/index - list all wares
      if (path === '/v1/index' && method === 'GET') {
        const list = await env.REGISTRY_BUCKET.list({ prefix: 'wares/' });
        const packages: string[] = [];
        
        for (const obj of list.objects) {
          if (obj.key?.endsWith('/index.json')) {
            const name = obj.key.replace('wares/', '').replace('/index.json', '');
            packages.push(name);
          }
        }
        
        return new Response(JSON.stringify(packages), {
          headers: { 'Content-Type': 'application/json', ...corsHeaders },
        });
      }

      // GET /v1/wares/:name - get ware info
      const waresMatch = path.match(/^\/v1\/wares\/([^/]+)$/);
      if (waresMatch && method === 'GET') {
        const name = waresMatch[1];
        const indexKey = `wares/${name}/index.json`;
        
        const index = await env.REGISTRY_BUCKET.get(indexKey);
        if (!index) {
          return new Response(JSON.stringify({ error: 'Ware not found' }), {
            status: 404,
            headers: { 'Content-Type': 'application/json', ...corsHeaders },
          });
        }
        
        return new Response(index.body, {
          headers: { 'Content-Type': 'application/json', ...corsHeaders },
        });
      }

      // GET /v1/wares/:name/:version - download ware
      const downloadMatch = path.match(/^\/v1\/wares\/([^/]+)\/([^/]+)$/);
      if (downloadMatch && method === 'GET') {
        const [_, name, version] = downloadMatch;
        const tarballKey = `wares/${name}/${version}.tarball`;
        
        const tarball = await env.REGISTRY_BUCKET.get(tarballKey);
        if (!tarball) {
          return new Response(JSON.stringify({ error: 'Version not found' }), {
            status: 404,
            headers: { 'Content-Type': 'application/json', ...corsHeaders },
          });
        }
        
        return new Response(tarball.body, {
          headers: { 
            'Content-Type': 'application/gzip',
            'Content-Disposition': `attachment; filename="${name}-${version}.tgz"`,
            ...corsHeaders 
          },
        });
      }

      // PUT /v1/wares - publish ware
      if (path === '/v1/wares' && method === 'PUT') {
        const body = await request.json();
        const { name, version, tarball, shasum } = body;
        
        if (!name || !version || !tarball) {
          return new Response(JSON.stringify({ error: 'Missing required fields: name, version, tarball' }), {
            status: 400,
            headers: { 'Content-Type': 'application/json', ...corsHeaders },
          });
        }

        // Upload tarball
        const tarballKey = `wares/${name}/${version}.tarball`;
        const tarballData = Uint8Array.from(atob(tarball), c => c.charCodeAt(0));
        await env.REGISTRY_BUCKET.put(tarballKey, tarballData, {
          httpMetadata: { contentType: 'application/gzip' },
        });

        // Update index
        const indexKey = `wares/${name}/index.json`;
        let index = { name, versions: [], latest: null };
        
        const existing = await env.REGISTRY_BUCKET.get(indexKey);
        if (existing) {
          index = await existing.json();
        }
        
        if (!index.versions.includes(version)) {
          index.versions.push(version);
          index.versions.sort((a: string, b: string) => {
            const [aMajor, aMinor, aPatch] = a.split('.').map(Number);
            const [bMajor, bMinor, bPatch] = b.split('.').map(Number);
            if (aMajor !== bMajor) return bMajor - aMajor;
            if (aMinor !== bMinor) return bMinor - aMinor;
            return bPatch - aPatch;
          });
          index.latest = index.versions[0];
        }
        
        await env.REGISTRY_BUCKET.put(indexKey, JSON.stringify(index), {
          httpMetadata: { contentType: 'application/json' },
        });

        return new Response(JSON.stringify({ success: true, name, version }), {
          status: 201,
          headers: { 'Content-Type': 'application/json', ...corsHeaders },
        });
      }

      // DELETE /v1/wares/:name/:version - yank ware
      if (downloadMatch && method === 'DELETE') {
        const [_, name, version] = downloadMatch;
        
        const indexKey = `wares/${name}/index.json`;
        const existing = await env.REGISTRY_BUCKET.get(indexKey);
        
        if (existing) {
          const index = await existing.json();
          index.yanked = index.yanked || {};
          index.yanked[version] = 'yanked by publisher';
          if (index.latest === version) {
            index.latest = index.versions.find((v: string) => v !== version && !index.yanked[v]);
          }
          await env.REGISTRY_BUCKET.put(indexKey, JSON.stringify(index));
        }
        
        return new Response(null, { status: 204, headers: corsHeaders });
      }

      // GET /v1/search - search wares
      if (path === '/v1/search' && method === 'GET') {
        const query = url.searchParams.get('q') || '';
        const limit = parseInt(url.searchParams.get('limit') || '20');
        
        const list = await env.REGISTRY_BUCKET.list({ prefix: 'wares/' });
        const results = [];
        
        for (const obj of list.objects) {
          if (obj.key?.endsWith('/index.json')) {
            const name = obj.key.replace('wares/', '').replace('/index.json', '');
            if (!query || name.toLowerCase().includes(query.toLowerCase())) {
              const index = await env.REGISTRY_BUCKET.get(obj.key);
              if (index) {
                const data = await index.json();
                results.push({ name, version: data.latest, description: data.description });
              }
            }
          }
          if (results.length >= limit) break;
        }
        
        return new Response(JSON.stringify({ packages: results, total: results.length }), {
          headers: { 'Content-Type': 'application/json', ...corsHeaders },
        });
      }

      return new Response(JSON.stringify({ 
        error: 'Not found',
        path,
        hint: 'Use /v1/index, /v1/wares/:name, /v1/wares/:name/:version, /v1/wares (PUT), /v1/search'
      }), {
        status: 404,
        headers: { 'Content-Type': 'application/json', ...corsHeaders },
      });

    } catch (e) {
      return new Response(JSON.stringify({ error: 'Internal error', details: String(e) }), {
        status: 500,
        headers: { 'Content-Type': 'application/json', ...corsHeaders },
      });
    }
  }
};
