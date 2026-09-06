/** The 2.0 browser journey: instruction -> application -> delayed assessment -> report. */
import { chromium } from 'playwright';
import { Run, checkMathRendered, checkNotBlank, finishWalk } from './checks.mjs';

const BASE = process.env.BASE ?? 'http://127.0.0.1:4173';
const SHOTS = process.env.SHOTS ?? './shots';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
const run = new Run(page, { shots: SHOTS, prefix: 'journey-' });

async function fillApplication({ work, final }) {
  await page.getByLabel('Divide total person-minutes by one worker\'s minutes.').check();
  await page.getByLabel('Step 1').fill(work);
  await page.getByLabel('Step 2').fill('240');
  await page.getByLabel('Final answer').fill(final);
  await page.locator('#integrated-reasoning').fill(
    'I divided total work by one worker\'s time.',
  );
}

async function main() {
  console.log('\n=== 2.0 integrated journey — deterministic browser fixture ===');
  await page.goto(`${BASE}/?demo=journey`, { waitUntil: 'domcontentloaded', timeout: 45000 });
  if (!(await checkNotBlank(run, '.view-dashboard .primary-action'))) return;

  await page.locator('.primary-action .btn-hero').click();
  await page.waitForSelector('.teach-card', { timeout: 25000 });
  run.note('approved instruction precedes the integrated application');
  await checkMathRendered(run, '.teach-card', 'the integrated-task instruction');
  await run.snap('instruction');

  await page.locator('.teach-card button').click();
  await page.waitForSelector('.integrated-task', { timeout: 25000 });
  const firstTitle = await run.text('.integrated-task .topic-name');
  run.note(`integrated application · "${firstTitle}"`);
  await checkMathRendered(run, '.integrated-scenario', 'the integrated application');
  const mathml = await run.count('.integrated-task .katex-mathml math');
  const presentation = await run.count('.integrated-task .katex-html[aria-hidden="true"]');
  if (mathml === 0 || presentation === 0) {
    run.fail(`rendered math accessibility: MathML=${mathml}, hidden presentation=${presentation}`);
  }
  run.note(`rendered math accessibility · ${mathml} MathML tree(s)`);

  await page.getByRole('button', { name: 'Hint (0/2)' }).click();
  await page.waitForSelector('.integrated-hint', { timeout: 15000 });
  const hint = await run.text('.integrated-hint');
  if (/1440|six workers|6 workers/i.test(hint)) run.fail(`hint revealed the answer: "${hint}"`);
  await fillApplication({ work: '1440', final: '6' });
  await page.getByRole('button', { name: 'Submit the whole task' }).click();
  await page.waitForSelector('.integrated-result', { timeout: 25000 });
  if (!(await run.text('.integrated-score')).includes('2 of 2 steps')) {
    run.fail(`unexpected application feedback: "${await run.text('.integrated-score')}"`);
  }
  run.note('step feedback and final interpretation rendered after submission');
  await run.snap('application-feedback');

  await page.getByRole('button', { name: 'Continue' }).click();
  await page.waitForSelector('.integrated-task', { timeout: 25000 });
  await page.waitForSelector('text=Delayed application assessment', { timeout: 25000 });
  const delayedTitle = await run.text('.integrated-task .topic-name');
  if (delayedTitle === firstTitle) run.fail('the delayed assessment repeated the studied item');
  if ((await run.count('.teach-card')) !== 0) run.fail('the delayed assessment repeated instruction');
  run.note(`unseen delayed assessment · "${delayedTitle}" · no repeated instruction`);
  await checkMathRendered(run, '.integrated-scenario', 'the delayed assessment');
  await fillApplication({ work: '1440', final: '6' });
  await page.getByRole('button', { name: 'Submit the whole task' }).click();
  await page.waitForSelector('.integrated-result', { timeout: 25000 });
  await run.snap('assessment-feedback');

  await page.getByRole('button', { name: 'Continue' }).click();
  await page.waitForSelector('.summary-card', { timeout: 25000 });
  run.note('journey completed and session closed');
  await page.getByRole('button', { name: 'Back to dashboard' }).click();
  await page.waitForSelector('.view-dashboard .primary-action', { timeout: 25000 });
  await page.locator('.more-menu summary').click();
  await page.getByRole('button', { name: 'Load the report' }).click();
  await page.waitForSelector('.retention-table', { timeout: 25000 });
  const report = await run.text('.retention-card');
  if (!report.includes('7 days later') || !report.includes('100%')) {
    run.fail(`the delayed result is absent from the retention report: "${report.slice(0, 180)}"`);
  }
  if (!report.includes('2 served, 2 passed')) {
    run.fail(`the integrated tally is wrong: "${report.slice(-180)}"`);
  }
  run.note('retention report shows the 7-day independent result and integrated tally');
  await run.snap('retention-report');
}

await finishWalk(main, run, browser);
