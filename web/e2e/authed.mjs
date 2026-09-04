/**
 * The authed click-through: a real account against the M5 `cadus-web` binary.
 *
 * `?demo=1` never touches the network, so this is the ONLY path that exercises the session
 * cookie, the CSRF origin layer, the security headers, and the service's own payloads. It
 * signs in, reads the dashboard the service builds, opens the curriculum map over the real
 * curriculum, and signs out.
 *
 * WHAT IT DOES NOT DO, and why that is not a gap. M5 mounts no `/api/diag/*` route
 * (`src/api/diag.ts`, `SPEC_ROUTES_ABSENT`), so the placement screen renders its stated
 * "not available" branch instead of a probe. This walk asserts THAT branch, because a walk
 * that skipped the screen would pass on the day the routes land and answer wrongly.
 *
 * `EMAIL` and `PASS` name an account `run.mjs` seeded. `BASE` names the origin.
 */
import { chromium } from 'playwright';
import { Run, checkNotBlank, finishWalk } from './checks.mjs';

const BASE = process.env.BASE ?? 'http://127.0.0.1:4173';
const SHOTS = process.env.SHOTS ?? './shots';
const EMAIL = process.env.EMAIL ?? 'click-through@cadus.local';
const PASS = process.env.PASS ?? 'a-long-enough-demo-password-2026';

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
const run = new Run(page, {
  shots: SHOTS,
  prefix: 'authed-',
  ignore: [
    // The boot `/api/auth/me` of a signed-out visitor answers 401, and that IS how the app
    // decides to show the auth card. Chrome logs a console line for it as well.
    /HTTP 401: .*\/api\/auth\/me/,
    /Failed to load resource.*401/,
    /status of 401/,
    // M5 mounts no `/api/diag/*` route (`src/api/diag.ts`, SPEC_ROUTES_ABSENT). The walk
    // ASSERTS that refusal below, so the 404 it produces is the expected answer, not a
    // finding. The day S10's Rust unit lands, the assertion below reports a probe instead
    // and this line stops matching anything.
    /HTTP 404: .*\/api\/diag\//,
    /Failed to load resource.*404/,
  ],
});

async function main() {
  console.log('\n=== a real account against the M5 service ===');
  await page.goto(BASE, { waitUntil: 'domcontentloaded', timeout: 45000 });

  if (!(await checkNotBlank(run, '.auth-view'))) return;
  run.note('auth card rendered for a signed-out visitor');
  await run.snap('login');

  // AUTH-7: no provider is configured on this deployment, so no OAuth button is offered.
  const oauth = await run.count('.oauth-btn');
  if (oauth !== 0) run.fail(`AUTH-7: ${oauth} OAuth buttons for a service that advertises none`);

  // The security headers the SERVICE stamps, read from the browser that has to obey them.
  //
  // Off `/api/health`, not off the document: `serve.mjs` is a test fixture and stamps
  // nothing, while the deployment puts Caddy in front of the same axum service (S14). The
  // API answer is the one response on this origin that carries the real header.
  const csp = await page.evaluate(async () => {
    const res = await fetch('/api/health');
    return res.headers.get('content-security-policy') ?? '';
  });
  run.note(`service CSP: "${csp.slice(0, 64)}…"`);
  if (!csp.startsWith("default-src 'self'")) {
    run.fail(`the service sent no usable Content-Security-Policy: "${csp}"`);
  }

  await page.locator('input[type="email"]').fill(EMAIL);
  await page.locator('input[type="password"]').fill(PASS);
  await page.locator('.auth-submit').click();

  // A cookie POST from a page the service does not call its own origin is
  // `403 cross_origin_rejected`. Reaching the dashboard at all proves the origin layer
  // accepted this one.
  await page.waitForSelector('.view-dashboard', { timeout: 30000 });
  await page.waitForSelector('.onboard-card, .primary-action', { timeout: 40000 });
  run.note(`signed in · topbar="${(await run.text('#topbar')).replace(/\n/g, ' ')}"`);
  await run.snap('dashboard');

  const cookies = await page.context().cookies();
  const session = cookies.find((c) => c.name.endsWith('cadus_session'));
  run.note(`session cookie: ${session ? `${session.name} httpOnly=${session.httpOnly}` : 'NONE'}`);
  if (!session) run.fail('the sign-in set no session cookie');
  // SEC-cookie: the credential lives in an HttpOnly cookie and nowhere the page can read.
  if (session && !session.httpOnly) run.fail('SEC-cookie: the session cookie is not HttpOnly');
  const stored = await page.evaluate(
    () => JSON.stringify({ ls: Object.keys(localStorage), ss: Object.keys(sessionStorage) }),
  );
  run.note(`web storage: ${stored}`);
  if (stored !== '{"ls":[],"ss":[]}') run.fail(`SEC-cookie: the page stored ${stored}`);

  // The curriculum map, over the REAL curriculum rather than the two synthetic topics.
  await page.locator('.topbar-link').click();
  await page.waitForSelector('.view-map', { timeout: 30000 });
  await page.waitForFunction(
    () => document.querySelectorAll('.map-canvas canvas').length > 0,
    null,
    { timeout: 30000 },
  );
  run.note(`real curriculum map · ${await run.text('.map-readout')}`);
  await run.snap('map');
  // The real curriculum is 1090 topics, and their layout runs on the main thread. Playwright
  // clicks only a STABLE element, so the control row has to settle first; the default
  // 30-second budget is not always enough for that layout on a loaded box.
  await page.getByRole('button', { name: 'Done' }).click({ timeout: 90000 });
  // Wait for the dashboard's CONTENT, never for its shell. `GET /api/status` is a round
  // trip over the whole curriculum, and `.view-dashboard` matches the spinner too: a read
  // taken here decides which dashboard is on screen from an empty one. Two of the four
  // false failures in 1.0's first run were exactly this.
  await page.waitForSelector('.onboard-card, .primary-action', { timeout: 40000 });

  // The placement. M5 mounts no `/api/diag/*` route, so the screen must say so rather than
  // spin. `Diagnostic.tsx` owns that branch and one unit test pins it.
  //
  // Two dashboards, and the walk reaches whichever this account has. An UNPLACED learner
  // gets the onboarding card — W-C3's one action — and no More menu at all; a placed one
  // gets the everyday layout with the placement under More.
  const onboard = await run.count('.onboard-card .btn-hero');
  if (onboard) {
    await page.locator('.onboard-card .btn-hero').click();
  } else {
    await page.locator('.more-menu summary').click();
    const placement = page.getByRole('button', { name: 'Re-run the placement' });
    await placement.waitFor({ state: 'visible', timeout: 10000 });
    await placement.click();
  }
  await page.waitForSelector('.view-diagnostic', { timeout: 25000 });
  run.note(onboard ? 'dashboard: onboarding (unplaced)' : 'dashboard: everyday (placed)');
  await page.getByRole('button', { name: 'Begin placement' }).click();
  await page.waitForSelector('.view-diagnostic .intro-error, .view-diagnostic .answer-input', {
    timeout: 30000,
  });
  const unavailable = await run.count('.intro-error');
  run.note(
    unavailable
      ? `placement refused, as M5 has no /api/diag route: "${await run.text('.intro-error')}"`
      : `probe 1 · "${await run.text('.problem-text')}"`,
  );
  await run.snap('placement');

  // Sign out — the one write that must clear the screen even when the service refuses.
  await page.locator('.brand').click();
  await page.waitForSelector('.onboard-card, .primary-action', { timeout: 40000 });
  await page.locator('.logout-btn').click();
  await page.waitForSelector('.auth-view', { timeout: 25000 });
  run.note('signed out, back at the auth card');
  await run.snap('signed-out');
}

await finishWalk(main, run, browser);
