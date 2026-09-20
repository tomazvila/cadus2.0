/**
 * Phone check of the answer buttons of a Label item (lane B7-spa), at 390 px.
 *
 * The demo lesson (`?demo=1`) has three typed problems, then one Label problem with the key
 * `choices`. This script goes to the Label problem and does these checks:
 *   1. The view shows the answer buttons and no typed field.
 *   2. Each button is one full-width row, and its height is 44 px or more.
 *   3. A long option and its math wrap: no button is wider than the card.
 *   4. The page has no horizontal scroll: `scrollWidth <= innerWidth`.
 *   5. A tap gives a verdict, and the buttons are disabled after the verdict.
 *   6. The verdict view marks the tapped option, and no other (`is-selected`, `aria-pressed`).
 *
 * Run it in the Playwright container, with the built bundle on `BASE`:
 *   npm run build && node e2e/serve.mjs --root=dist --port=4174 &
 *   docker run --rm -v "$PWD/e2e:/out" -w /out --user "$(id -u):$(id -g)" --network host \
 *     -e HOME=/out -e PLAYWRIGHT_BROWSERS_PATH=/ms-playwright -e BASE=http://127.0.0.1:4174 \
 *     -e SHOTS=/out/shots/phone-choices mcr.microsoft.com/playwright:v1.62.1-noble \
 *     node /out/phone-choices.mjs
 */
import { mkdirSync } from 'node:fs';
import { chromium } from 'playwright';

const BASE = process.env.BASE ?? 'http://127.0.0.1:4174';
const SHOTS = process.env.SHOTS ?? 'shots/phone-choices';
const WIDTH = 390;
const MIN_TARGET_PX = 44;
const WAIT = { timeout: 25000 };

/** The answers of the three typed demo problems, by the position in the lesson. */
const TYPED_ANSWERS = { 1: '3/4', 2: '4', 3: '7' };
/** The correct option of the demo Label problem (`src/api/demo.ts`, `demo-p4`). */
const CORRECT_OPTION = 'Step 2: divide each side by 2';

mkdirSync(SHOTS, { recursive: true });
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: WIDTH, height: 844 } });
const fails = [];

async function noHorizontalScroll(label) {
  const size = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    innerWidth: window.innerWidth,
  }));
  console.log(`${label}: scrollWidth=${size.scrollWidth} innerWidth=${size.innerWidth}`);
  if (size.scrollWidth > size.innerWidth) {
    fails.push(`${label}: horizontal scroll (${size.scrollWidth} > ${size.innerWidth})`);
  }
}

/** Do one step of the lesson. Return true when the answer buttons are on the screen. */
async function step() {
  await page.waitForSelector('.teach-card, .problem-card', WAIT);
  if (await page.locator('.teach-card').count()) {
    await page.locator('.teach-card button').click();
    await page.waitForSelector('.problem-card', WAIT);
  }
  await page.waitForSelector('.choice-buttons, .answer-input:not([disabled])', WAIT);
  if (await page.locator('.choice-buttons').count()) return true;

  const position = (await page.locator('.progress-count').textContent()).trim();
  const answer = TYPED_ANSWERS[Number(position.split('/')[0])];
  console.log(`typed problem ${position}: answer ${answer}`);
  await page.locator('.answer-input').fill(answer);
  await page.getByRole('button', { name: 'Submit', exact: true }).click();
  await page.waitForSelector('.feedback .btn-primary', WAIT);
  await page.locator('.feedback .btn-primary').click();
  // Wait for the move: the teach card of the next knowledge point, or the next problem.
  await page.waitForFunction((previous) => {
    if (document.querySelector('.teach-card')) return true;
    const count = document.querySelector('.progress-count');
    return count !== null && count.textContent.trim() !== previous;
  }, position, WAIT);
  return false;
}

await page.goto(`${BASE}/?demo=1`, { waitUntil: 'domcontentloaded', timeout: 45000 });
await page.waitForSelector('.view-dashboard .primary-action', WAIT);
await page.locator('.primary-action .btn-hero').click();

let onLabel = false;
for (let i = 0; i < 6 && !onLabel; i += 1) onLabel = await step();
if (!onLabel) fails.push('the Label problem did not show answer buttons');

if (onLabel) {
  const group = page.getByRole('group', { name: 'Answer choices' });
  const buttons = group.getByRole('button');
  const count = await buttons.count();
  console.log(`Label problem ${(await page.locator('.progress-count').textContent()).trim()}: ${count} buttons`);
  if (count !== 4) fails.push(`the Label problem shows ${count} buttons, not 4`);
  if (await page.locator('.answer-input').count()) fails.push('the typed field is on the screen');

  const card = await page.locator('.choice-buttons').boundingBox();
  let lastBottom = -1;
  for (let i = 0; i < count; i += 1) {
    const box = await buttons.nth(i).boundingBox();
    const text = (await buttons.nth(i).textContent()).trim();
    console.log(`button ${i + 1}: ${Math.round(box.width)}x${Math.round(box.height)} px, ${text.length} characters`);
    if (box.height < MIN_TARGET_PX) fails.push(`button ${i + 1} is ${box.height} px high`);
    if (Math.abs(box.width - card.width) > 1) fails.push(`button ${i + 1} is not a full-width row`);
    if (box.x + box.width > WIDTH) fails.push(`button ${i + 1} goes out of the viewport`);
    if (box.y < lastBottom) fails.push(`button ${i + 1} is not below button ${i}`);
    lastBottom = box.y + box.height;
  }
  const mathCount = await group.locator('.katex').count();
  console.log(`math elements in the buttons: ${mathCount}`);
  if (mathCount === 0) fails.push('no option shows rendered math');
  await noHorizontalScroll('label-ready');
  await page.screenshot({ path: `${SHOTS}/label-ready.png`, fullPage: true });

  await group.getByRole('button', { name: CORRECT_OPTION }).click();
  await page.waitForSelector('.feedback .feedback-title', WAIT);
  const verdict = (await page.locator('.feedback .feedback-title').first().textContent()).trim();
  console.log(`verdict: ${verdict}`);
  if (verdict !== 'Correct') fails.push(`the verdict reads "${verdict}", not "Correct"`);
  const disabled = await buttons.evaluateAll((all) => all.every((b) => b.disabled));
  if (!disabled) fails.push('the buttons are not disabled after the verdict');
  const marked = await buttons.evaluateAll((all) => all
    .filter((b) => b.classList.contains('is-selected') && b.getAttribute('aria-pressed') === 'true')
    .map((b) => ({ text: b.textContent.trim(), border: getComputedStyle(b).borderTopWidth, opacity: getComputedStyle(b).opacity })));
  console.log(`marked options: ${JSON.stringify(marked)}`);
  if (marked.length !== 1 || !marked[0].text.startsWith('Step 2')) fails.push('the verdict view does not mark the tapped option only');
  else if (marked[0].border !== '2px' || marked[0].opacity !== '1') fails.push('the mark of the tapped option has no visible style');
  await noHorizontalScroll('label-verdict');
  await page.screenshot({ path: `${SHOTS}/label-verdict.png`, fullPage: true });
}

await browser.close();
if (fails.length) { console.log(`FAIL:\n${fails.join('\n')}`); process.exit(1); }
console.log('PHONE CHOICES OK');
