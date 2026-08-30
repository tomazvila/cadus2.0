#!/usr/bin/env node
/**
 * The click-through driver.
 *
 *   node e2e/run.mjs demo                     one walk against a clean build
 *   node e2e/run.mjs authed --api=URL         one walk against the M5 service
 *   node e2e/run.mjs acceptance               the S13 acceptance check
 *
 * `acceptance` is the check the unit is measured by: it builds each of the three
 * deliberately broken trees of `breaks.mjs`, runs the demo walk against each, and requires
 * the matching failure; then it builds the real tree and requires a clean pass.
 *
 * THE BROWSER RUNS IN A CONTAINER, ALWAYS. Playwright's browsers install on this box
 * without root but do not launch: `chrome-headless-shell` needs system libraries and
 * `playwright install-deps` needs root, which this box has none of (spec section 7.3). The
 * Playwright image carries its own. Nothing here ever launches a browser natively.
 *
 * Everything it writes stays under `web/e2e/` — `work/` for the broken copies, `shots/` for
 * the screenshots, `node_modules/` for the container's Playwright. All three are ignored by
 * git.
 */
import { spawn, spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { BREAKS } from './breaks.mjs';

const e2e = dirname(fileURLToPath(import.meta.url));
const web = resolve(e2e, '..');
const work = join(e2e, 'work');
const shots = join(e2e, 'shots');

const IMAGE = 'mcr.microsoft.com/playwright:v1.62.1-noble';
const PLAYWRIGHT = 'playwright@1.62.1';

const command = process.argv[2] ?? 'demo';
const arg = (name, fallback) => {
  const hit = process.argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : fallback;
};
const port = Number(arg('port', '4173'));
const api = arg('api', '');

/** Everything the build needs, and nothing that would recurse or bloat the copy. */
const SKIP = new Set(['node_modules', 'dist', 'e2e', '.git', 'coverage']);

function run(cmd, args, opts = {}) {
  const res = spawnSync(cmd, args, { stdio: 'inherit', ...opts });
  if (res.status !== 0) throw new Error(`${cmd} ${args.join(' ')} exited ${String(res.status)}`);
}

/** Copy the source tree, apply one break, and build. Returns the `dist/` to serve. */
function buildVariant(id, apply) {
  const dir = join(work, id);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(work, { recursive: true });
  mkdirSync(dir, { recursive: true });
  // ENTRY BY ENTRY, not one `cpSync(web, dir)`. The destination is inside the source —
  // it has to be, or node cannot resolve Vite from `web/node_modules` — and `cpSync`
  // refuses that outright, filter or no filter.
  for (const entry of readdirSync(web)) {
    if (SKIP.has(entry)) continue;
    cpSync(join(web, entry), join(dir, entry), { recursive: true });
  }
  if (apply) apply(dir);
  // `vite build` only. `tsc -b` is the unit gate's job, and a broken variant is allowed to
  // be a build the type gate would refuse — the point is what the BROWSER does with it.
  run('node', [join(web, 'node_modules/vite/bin/vite.js'), 'build'], { cwd: dir });
  return join(dir, 'dist');
}

/** Build the real tree in place, through the package script the gate runs. */
function buildClean() {
  run('npm', ['run', 'build'], { cwd: web });
  return join(web, 'dist');
}

/** Start `serve.mjs` on `dist`, and resolve once it answers. */
async function serve(dist) {
  const child = spawn(
    'node',
    [join(e2e, 'serve.mjs'), `--root=${dist}`, `--port=${String(port)}`, ...(api ? [`--api=${api}`] : [])],
    { stdio: 'inherit' },
  );
  for (let i = 0; i < 60; i += 1) {
    try {
      const res = await fetch(`http://127.0.0.1:${String(port)}/index.html`);
      if (res.ok) return child;
    } catch { /* not up yet */ }
    await new Promise((r) => { setTimeout(r, 250); });
  }
  child.kill('SIGTERM');
  throw new Error(`serve.mjs never answered on port ${String(port)}`);
}

/**
 * How the container reaches the server, decided ONCE by asking it.
 *
 * The Playwright image's own documentation says `host.docker.internal`, and
 * `--add-host=…:host-gateway` is what makes that name resolve on Linux. On a host whose
 * firewall drops traffic from the docker bridge the name resolves and the connection still
 * hangs — which shows up as a 45-second `page.goto` timeout and nothing else, so the mode
 * is PROBED rather than assumed. `--network host` is the fallback: it puts the browser in
 * the host's own network namespace, where loopback is the same loopback.
 *
 * `E2E_NETWORK=bridge` or `=host` skips the probe.
 */
let network = null;

function pickNetwork() {
  if (network) return network;
  const forced = process.env.E2E_NETWORK;
  const bridge = {
    args: ['--add-host', 'host.docker.internal:host-gateway'],
    base: `http://host.docker.internal:${String(port)}`,
    how: 'the docker bridge, through host.docker.internal',
  };
  const host = {
    args: ['--network', 'host'],
    base: `http://127.0.0.1:${String(port)}`,
    how: "the host's own network namespace",
  };
  if (forced === 'bridge') { network = bridge; return network; }
  if (forced === 'host') { network = host; return network; }

  const probe = spawnSync('docker', [
    'run', '--rm', ...bridge.args, IMAGE,
    'sh', '-c', `curl -s -o /dev/null --max-time 6 ${bridge.base}/index.html`,
  ], { encoding: 'utf8' });
  network = probe.status === 0 ? bridge : host;
  console.log(`click-through network: ${network.how}`);
  return network;
}

/**
 * Run one walk in the Playwright container.
 *
 * `--user` keeps every file the container writes — `node_modules/`, `shots/` — owned by
 * the caller rather than by root. `--no-save` keeps the install out of `package.json`,
 * which pins the version this image ships with.
 *
 * Returns `{ code, output }`; the output is captured so the acceptance check can require
 * the RIGHT failure rather than any failure.
 */
function walk(script, env = {}) {
  mkdirSync(shots, { recursive: true });
  const net = pickNetwork();
  const install = existsSync(join(e2e, 'node_modules/playwright'))
    ? ''
    : `npm i ${PLAYWRIGHT} --silent --no-save --no-audit --no-fund && `;
  const args = [
    'run', '--rm',
    '-v', `${e2e}:/out`,
    '-w', '/out',
    '--user', `${String(process.getuid?.() ?? 0)}:${String(process.getgid?.() ?? 0)}`,
    ...net.args,
    '-e', 'HOME=/out',
    '-e', 'PLAYWRIGHT_BROWSERS_PATH=/ms-playwright',
    '-e', `BASE=${net.base}`,
    '-e', 'SHOTS=/out/shots',
  ];
  for (const [k, v] of Object.entries(env)) args.push('-e', `${k}=${v}`);
  args.push(IMAGE, 'sh', '-c', `${install}node ${script}`);

  const res = spawnSync('docker', args, { encoding: 'utf8' });
  const output = `${res.stdout ?? ''}${res.stderr ?? ''}`;
  process.stdout.write(output);
  return { code: res.status ?? 1, output };
}

/** One walk against one build, with the server up for exactly that walk. */
async function against(dist, script, env) {
  const server = await serve(dist);
  try {
    return walk(script, env);
  } finally {
    server.kill('SIGTERM');
    // The next variant binds the same port, and a socket still in TIME_WAIT refuses it.
    await new Promise((r) => { setTimeout(r, 400); });
  }
}

async function acceptance() {
  const results = [];

  for (const brk of BREAKS) {
    console.log(`\n### BROKEN BUILD "${brk.id}" — ${brk.what}`);
    const dist = buildVariant(brk.id, brk.apply);
    const { code, output } = await against(dist, 'demo.mjs');
    const caught = output.includes(brk.expect);
    results.push({
      id: brk.id,
      // Both clauses. A run that exits non-zero for an unrelated reason has not caught
      // this failure, and a run that names it while exiting 0 has not failed on it.
      pass: code !== 0 && caught,
      detail: `exit=${String(code)} reported="${brk.expect}"=${String(caught)}`,
    });
  }

  console.log('\n### CLEAN BUILD — every flow must pass');
  const { code } = await against(buildClean(), 'demo.mjs');
  results.push({ id: 'clean', pass: code === 0, detail: `exit=${String(code)}` });

  console.log('\n================ S13 ACCEPTANCE ================');
  for (const r of results) {
    console.log(`  ${r.pass ? 'PASS' : 'FAIL'}  ${r.id.padEnd(14)} ${r.detail}`);
  }
  const ok = results.every((r) => r.pass);
  console.log(ok
    ? 'ACCEPTANCE PASS: each of the three 1.0 failures is reproduced and caught; a clean '
      + 'build passes all flows.'
    : 'ACCEPTANCE FAIL.');
  return ok ? 0 : 1;
}

let exit = 0;
if (command === 'acceptance') {
  exit = await acceptance();
} else if (command === 'demo') {
  const dist = process.argv.includes('--no-build') ? join(web, 'dist') : buildClean();
  exit = (await against(dist, 'demo.mjs')).code;
} else if (command === 'authed') {
  if (!api) throw new Error('authed needs --api=http://127.0.0.1:8080');
  const dist = process.argv.includes('--no-build') ? join(web, 'dist') : buildClean();
  exit = (await against(dist, 'authed.mjs', {
    EMAIL: process.env.EMAIL ?? 'click-through@cadus.local',
    PASS: process.env.PASS ?? 'a-long-enough-demo-password-2026',
  })).code;
} else {
  throw new Error(`unknown command "${command}" — use demo, authed, or acceptance`);
}
process.exit(exit);
