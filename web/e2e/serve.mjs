#!/usr/bin/env node
/**
 * The origin the click-through loads: `dist/` as static files, `/api` proxied to the axum
 * service, both on ONE origin.
 *
 * WHY THIS EXISTS AND NOT `vite preview`. The deployment serves the built bundle and the
 * API from the same origin (Caddy, S14), and the service refuses a cross-origin cookie
 * write with `403 cross_origin_rejected`. A preview server on one port with the API on
 * another makes every authed POST fail for a reason the product does not have. This server
 * is the smallest thing that reproduces the deployed shape: no dependencies, no rewriting,
 * one origin.
 *
 * It is a TEST fixture. It is not the deployment, it adds no security headers, and nothing
 * outside `e2e/` imports it.
 *
 *   node e2e/serve.mjs --root=dist --port=4173 [--api=http://127.0.0.1:8080]
 *
 * The SPA fallback serves `index.html` for any path that is not a file, because `/ops` and
 * `/review` are real URLs the operator reloads.
 */
import { createServer } from 'node:http';
import { createReadStream, existsSync, statSync } from 'node:fs';
import { extname, join, normalize, resolve } from 'node:path';

const arg = (name, fallback) => {
  const hit = process.argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : fallback;
};

const root = resolve(arg('root', 'dist'));
const port = Number(arg('port', '4173'));
const api = arg('api', '');

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.woff2': 'font/woff2',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.txt': 'text/plain; charset=utf-8',
};

/** The file a URL path names, or null when it names none. */
function fileFor(pathname) {
  // `normalize` collapses `..`, and the prefix test then keeps every answer inside the
  // root. A path that escapes it is not served, whatever it decodes to.
  const wanted = normalize(join(root, decodeURIComponent(pathname)));
  if (!wanted.startsWith(root)) return null;
  if (existsSync(wanted) && statSync(wanted).isFile()) return wanted;
  return null;
}

/** Forward one `/api` request to the service, headers, body, cookies and all. */
async function proxy(req, res) {
  if (!api) {
    res.writeHead(502, { 'content-type': 'application/json' });
    res.end('{"error":{"code":"no_api","message":"serve.mjs was started with no --api"}}');
    return;
  }
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const headers = { ...req.headers };
  // The service compares `Origin` against PUBLIC_ORIGIN, so the header travels VERBATIM.
  // Only `host` is rewritten, because it names this hop.
  delete headers.host;
  delete headers['content-length'];
  const body = chunks.length ? Buffer.concat(chunks) : undefined;
  try {
    const upstream = await fetch(`${api}${req.url}`, {
      method: req.method,
      headers,
      body: req.method === 'GET' || req.method === 'HEAD' ? undefined : body,
      redirect: 'manual',
    });
    const out = Object.fromEntries(upstream.headers.entries());
    // `getSetCookie` keeps the cookies SEPARATE. Folded into one comma-joined header they
    // reach the browser as one malformed cookie and the session never opens.
    const cookies = upstream.headers.getSetCookie?.() ?? [];
    delete out['set-cookie'];
    delete out['content-encoding'];
    delete out['content-length'];
    res.writeHead(upstream.status, cookies.length ? { ...out, 'set-cookie': cookies } : out);
    res.end(Buffer.from(await upstream.arrayBuffer()));
  } catch (e) {
    res.writeHead(502, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ error: { code: 'upstream', message: String(e) } }));
  }
}

const server = createServer((req, res) => {
  const pathname = (req.url ?? '/').split('?')[0];
  if (pathname.startsWith('/api/')) { void proxy(req, res); return; }

  const file = fileFor(pathname) ?? join(root, 'index.html');
  if (!existsSync(file)) { res.writeHead(404); res.end('not found'); return; }
  res.writeHead(200, {
    'content-type': TYPES[extname(file)] ?? 'application/octet-stream',
    // The click-through builds a fresh bundle per run and reloads inside one run. A cached
    // entry chunk from the previous build would report the previous build's verdict.
    'cache-control': 'no-store',
  });
  createReadStream(file).pipe(res);
});

// 0.0.0.0, because the browser runs in a container and reaches this through the host
// gateway. A loopback bind answers nothing from there.
server.listen(port, '0.0.0.0', () => {
  console.log(`serve.mjs: ${root} on http://0.0.0.0:${port}${api ? ` · /api → ${api}` : ''}`);
});
