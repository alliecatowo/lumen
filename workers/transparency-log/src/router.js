/**
 * Simple HTTP Router for Cloudflare Workers
 */

export class Router {
  constructor() {
    this.routes = [];
  }

  get(path, handler) {
    this.routes.push({ method: 'GET', path, handler });
    return this;
  }

  post(path, handler) {
    this.routes.push({ method: 'POST', path, handler });
    return this;
  }

  put(path, handler) {
    this.routes.push({ method: 'PUT', path, handler });
    return this;
  }

  delete(path, handler) {
    this.routes.push({ method: 'DELETE', path, handler });
    return this;
  }

  patch(path, handler) {
    this.routes.push({ method: 'PATCH', path, handler });
    return this;
  }

  async handle(request) {
    const url = new URL(request.url);
    const pathname = url.pathname;
    const method = request.method;

    // Handle CORS preflight
    if (method === 'OPTIONS') {
      return new Response(null, {
        status: 204,
        headers: {
          'Access-Control-Allow-Origin': '*',
          'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, PATCH, OPTIONS',
          'Access-Control-Allow-Headers': 'Content-Type, Authorization, X-API-Key',
        },
      });
    }

    for (const route of this.routes) {
      const match = this.matchPath(route.path, pathname);
      if (route.method === method && match) {
        try {
          return await route.handler(request, match.params);
        } catch (error) {
          console.error('Route handler error:', error);
          return new Response(JSON.stringify({ error: 'Internal server error' }), {
            status: 500,
            headers: { 'Content-Type': 'application/json' },
          });
        }
      }
    }

    return new Response(JSON.stringify({ error: 'Not found' }), {
      status: 404,
      headers: { 
        'Content-Type': 'application/json',
        'Access-Control-Allow-Origin': '*',
      },
    });
  }

  matchPath(routePath, actualPath) {
    // Convert route pattern to regex
    const paramNames = [];
    let regexPattern = routePath
      .replace(/:([^/]+)/g, (match, name) => {
        paramNames.push(name);
        return '([^/]+)';
      })
      .replace(/\*/g, '.*');

    regexPattern = '^' + regexPattern + '$';
    const regex = new RegExp(regexPattern);
    const match = actualPath.match(regex);

    if (!match) {
      return null;
    }

    const params = {};
    paramNames.forEach((name, index) => {
      params[name] = match[index + 1];
    });

    return { params };
  }
}
