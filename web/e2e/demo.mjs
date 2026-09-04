/**
 * The demo click-through: `?demo=1`, the canned backend, no account and no network.
 *
 * WHY A BROWSER AT ALL. The unit suite runs in jsdom, which cannot see the three things
 * most likely to be wrong in a real browser: an entry chunk that never evaluates, a KaTeX
 * pass that never ran, and a Cytoscape canvas that never painted. This walk drives the
 * BUILT bundle in Chromium and records every console error, page error, failed request and
 * 4xx/5xx along the way.
 *
 * Every literal below is `src/api/demo.ts` and `src/api/diag.ts`, which is what makes the
 * walk an assertion rather than a smoke test.
 *
 * `BASE` names the origin the server is on. `SHOTS` is where the screenshots land.
 */
import { chromium } from 'playwright';
import {
  Run, checkMathRendered, checkNotBlank, checkSameProblem, finishWalk,
} from './checks.mjs';

const BASE = process.env.BASE ?? 'http://127.0.0.1:4173';
const SHOTS = process.env.SHOTS ?? './shots';

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
const run = new Run(page, { shots: SHOTS, prefix: 'demo-' });

/** The lesson, from the dashboard to the first practice problem. */
async function intoLesson() {
  await page.locator('.primary-action .btn-hero').click();
  await page.waitForSelector('.teach-card', { timeout: 25000 });
  await page.locator('.teach-card button').click();
  await page.waitForSelector('.problem-card .answer-input', { timeout: 25000 });
  return {
    position: await run.text('.progress-count'),
    text: await run.text('.problem-text'),
  };
}

/** Back to the dashboard through the brand, and wait for its content. */
async function home() {
  await page.locator('.brand').click();
  await page.waitForSelector('.view-dashboard .primary-action', { timeout: 25000 });
}

async function main() {
  console.log('\n=== ?demo=1 — the canned backend, no account ===');
  await page.goto(`${BASE}/?demo=1`, { waitUntil: 'domcontentloaded', timeout: 45000 });

  // FAILURE 1. Nothing below this line can run on a blank page, so the walk stops here.
  if (!(await checkNotBlank(run, '.view-dashboard .primary-action'))) return;
  run.note(`dashboard rendered · topbar="${(await run.text('#topbar')).replace(/\n/g, ' ')}"`);
  await run.snap('dashboard');

  if ((await run.count('.demo-badge')) !== 1) run.fail('the topbar shows no DEMO badge');
  // W-C2: exactly one primary action on the dashboard.
  const heroes = await run.count('.primary-action .btn-hero');
  if (heroes !== 1) run.fail(`W-C2: the dashboard offers ${heroes} primary actions, not 1`);

  // --- the curriculum map: Cytoscape must actually paint a canvas ------------
  await page.locator('.topbar-link').click();
  await page.waitForSelector('.view-map', { timeout: 25000 });
  await page.waitForFunction(
    () => document.querySelectorAll('.map-canvas canvas').length > 0,
    null,
    { timeout: 30000 },
  );
  const readout = await run.text('.map-readout');
  run.note(`map painted · ${await run.count('.map-canvas canvas')} canvases · readout="${readout}"`);
  // The demo graph: two topics, one link, one mastered.
  if (readout !== '2 topics · 1 links · 1 mastered') {
    run.fail(`the map readout is "${readout}", not "2 topics · 1 links · 1 mastered"`);
  }
  await run.snap('map');

  await page.getByRole('button', { name: 'List view' }).click();
  await page.waitForSelector('.map-list', { timeout: 15000 });
  const rows = await run.count('.map-list li');
  run.note(`list view · ${rows} rows`);
  if (rows !== 2) run.fail(`the accessible list holds ${rows} rows, not 2`);
  await run.snap('map-list');

  await page.getByRole('button', { name: 'Done' }).click();
  await page.waitForSelector('.view-dashboard .primary-action', { timeout: 25000 });

  // --- a lesson: teach, then practise, and KaTeX must have run ---------------
  await page.locator('.primary-action .btn-hero').click();
  await page.waitForSelector('.teach-card', { timeout: 25000 });
  run.note(`worked example shown · ${await run.count('.teach-card .katex')} KaTeX nodes`);
  await run.snap('lesson-teach');
  // FAILURE 2, first sighting: the worked example is the first math on screen.
  await checkMathRendered(run, '.teach-card', 'the worked example');

  await page.locator('.teach-card button').click();
  await page.waitForSelector('.problem-card .answer-input', { timeout: 25000 });
  const first = {
    position: await run.text('.progress-count'),
    text: await run.text('.problem-text'),
  };
  run.note(`practice problem ${first.position} · "${first.text}"`);
  await run.snap('lesson-practice');
  await checkMathRendered(run, '.problem-text', 'the problem statement');
  if (first.position !== '1 / 3') {
    run.fail(`the lesson opened at ${first.position}, not at 1 / 3`);
  }

  // --- FAILURE 3: two serves in a row must serve the same problem -----------
  //
  // FIRST, and before anything else touches the lesson. A backend that advances a cursor
  // per serve is out of step with the learner from the second call onward, and every step
  // after it then fails for a reason that hides this one.
  await page.locator('.btn-exit').click();
  await page.waitForSelector('.view-dashboard .primary-action', { timeout: 25000 });
  run.note('left the lesson for the dashboard');
  const again = await intoLesson();
  run.note(`re-entered the lesson at ${again.position} · "${again.text}"`);
  checkSameProblem(run, first, again);
  await run.snap('lesson-reentered');

  // Hard Rule 1: a hint never carries the expected answer.
  await page.getByRole('button', { name: 'Hint' }).click();
  await page.waitForSelector('.hint', { timeout: 20000 });
  const hint = await run.text('.hint');
  run.note(`hint shown: "${hint.slice(0, 70)}"`);
  if (hint.includes('3/4')) run.fail(`Hard Rule 1: the hint carries the answer — "${hint}"`);

  // --- the grade, its solution, and the auto-advance -------------------------
  await page.locator('.answer-input').fill('3/4');
  await page.locator('.actions .btn-primary').click();
  await page.waitForSelector('.feedback-correct', { timeout: 25000 });
  run.note(`graded · ${(await run.text('.feedback-head')).replace(/\n/g, ' ')}`);
  await checkMathRendered(run, '.solution-text', 'the worked solution');
  await run.snap('lesson-feedback');

  // Auto-advance is 1400 ms, correct answers only. Problem 2 belongs to a knowledge point
  // this lesson has not taught, so the lesson teaches it before it practises it.
  await page.waitForSelector('.teach-card', { timeout: 20000 });
  await page.locator('.teach-card button').click();
  await page.waitForSelector('.problem-card .answer-input', { timeout: 25000 });
  const second = await run.text('.progress-count');
  run.note(`auto-advanced to ${second}`);
  if (second !== '2 / 3') run.fail(`the second problem reads ${second}, not 2 / 3`);

  // --- the placement: three ground rules, and no solution, ever --------------
  await home();
  await page.locator('.more-menu summary').click();
  const placement = page.getByRole('button', { name: 'Re-run the placement' });
  await placement.waitFor({ state: 'visible', timeout: 10000 });
  await placement.click();
  await page.waitForSelector('.view-diagnostic .intro-card', { timeout: 25000 });
  const groundRules = await run.count('.intro-rules li');
  run.note(`placement intro · ${groundRules} ground rules`);
  if (groundRules !== 3) run.fail(`P3: the intro lists ${groundRules} ground rules, not 3`);
  await run.snap('placement-intro');

  await page.getByRole('button', { name: 'Begin placement' }).click();
  await page.waitForSelector('.view-diagnostic .answer-input', { timeout: 25000 });
  run.note(`probe 1 · "${await run.text('.problem-text')}"`);
  await page.locator('.answer-input').fill('5');
  await page.locator('.actions .btn-primary').click();
  await page.waitForSelector('.feedback-mark', { timeout: 25000 });
  run.note(`probe graded · ${(await run.text('.feedback-title'))}`);
  // DIAG-nosol: a tick or a cross, and nothing else. Never a solution, never an expected
  // answer.
  if ((await run.count('.view-diagnostic .solution')) > 0) {
    run.fail('DIAG-nosol: the placement rendered a solution panel');
  }
  await run.snap('placement-graded');

  // --- responsive ------------------------------------------------------------
  console.log('\n=== responsive ===');
  await page.setViewportSize({ width: 390, height: 844 });
  await home();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  run.note(`390px wide · horizontal overflow=${overflow}px`);
  await run.snap('mobile-dashboard');
  if (overflow > 2) run.fail(`the page scrolls horizontally at 390px (${overflow}px)`);
}

await finishWalk(main, run, browser);
