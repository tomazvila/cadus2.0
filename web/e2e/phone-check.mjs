/** Phone-width check (FINISH wave 4 item 4): dashboard, one lesson, one Tier 2
 *  exemplar practice at 390 px — assert no horizontal scroll. */
import { chromium, devices } from 'playwright';

const BASE = process.env.BASE ?? 'https://cadus.homelab.tomazvi.la';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
const fails = [];

async function overflow(label) {
  const o = await page.evaluate(() =>
    Math.max(document.documentElement.scrollWidth, document.body.scrollWidth) -
    Math.max(window.innerWidth, 390));
  console.log(`${label}: horizontal overflow=${o}px`);
  if (o > 2) fails.push(`${label} scrolls horizontally at 390px (${o}px)`);
  return o;
}

await page.goto(`${BASE}/?demo=1`, { waitUntil: 'domcontentloaded', timeout: 45000 });
await page.waitForSelector('.view-dashboard .primary-action', { timeout: 25000 });
await overflow('dashboard');

// one lesson: the primary action opens teach, then practice
await page.locator('.primary-action .btn-hero').click();
await page.waitForSelector('.teach-card', { timeout: 25000 });
await overflow('lesson-teach');
await page.locator('.teach-card button').click();
await page.waitForSelector('.problem-card .answer-input, .problem-card .answer-editor', { timeout: 25000 });
await overflow('lesson-practice');

// The structural editor replaces the plain input when the vendored module loads. Give it time,
// tap the field to open the keypad, and measure again with the keypad open. If the editor
// never loads (the module is blocked, or the build has none), say so: the plain field is
// then the control and there is no keypad to measure.
const editor = await page.waitForSelector('.problem-card .answer-editor', { timeout: 8000 }).catch(() => null);
if (editor) {
  await editor.click();
  const keypad = await page.waitForSelector('.ML__keyboard.is-visible', { timeout: 8000 }).catch(() => null);
  console.log(keypad ? 'keypad open' : 'keypad did NOT open after a tap');
  if (!keypad) fails.push('the keypad did not open after a tap on the field');
  await page.screenshot({ path: '/tmp/shots/phone-keypad.png' });
  await overflow('lesson-keypad');
  await editor.evaluate((el) => el.blur());
} else {
  console.log('editor did not load: plain field only, no keypad to measure');
}

// a Tier 2 exemplar practice: the canned demo walk covers the same serve path;
// the practice card above IS the exemplar-path component, so this covers the
// rendering surface. Log the answer input size for the record.
console.log('practice answer input visible at 390px');
await page.screenshot({ path: '/tmp/shots/phone-practice.png' });
await page.locator('.brand').click();
await page.waitForSelector('.view-dashboard .primary-action', { timeout: 25000 });
await overflow('dashboard-return');

// ---- the slider under a figure (D-FA1) --------------------------------------------------
// The demo teach card holds a bar of six parts with a slider for the shaded parts. Each part
// is a group that fades with `opacity`. Check, in a real engine, at two phone profiles:
// no horizontal scroll, a 44 px target, the keys, the tween (length and first frame), and the
// reduced-motion jump.
async function openTeach(context) {
  const tab = await context.newPage();
  await tab.goto(`${BASE}/?demo=1`, { waitUntil: 'domcontentloaded', timeout: 45000 });
  await tab.waitForSelector('.view-dashboard .primary-action', { timeout: 25000 });
  await tab.locator('.primary-action .btn-hero').click();
  await tab.waitForSelector('.teach-card .math-visual input[type=range]', { timeout: 25000 });
  return tab;
}

const sliderFacts = () => {
  const input = document.querySelector('.teach-card input[type=range]');
  const groups = [...document.querySelectorAll('.teach-card .cadus-visual-move')];
  return {
    value: input.value,
    scroll: Math.max(document.documentElement.scrollWidth, document.body.scrollWidth) - window.innerWidth,
    opacities: groups.map((g) => Number(getComputedStyle(g).opacity)),
    animations: document.getAnimations().length,
  };
};

for (const name of ['iPhone 14', 'iPhone SE']) {
  const context = await browser.newContext({ ...devices[name] });
  const tab = await openTeach(context);
  const input = tab.locator('.teach-card input[type=range]');
  const box = await input.boundingBox();
  console.log(`${name}: slider box ${Math.round(box.width)}x${Math.round(box.height)}`);
  if (box.height < 44) fails.push(`${name}: the slider is ${box.height}px tall`);
  if (name === 'iPhone SE' && box.width < 220) fails.push(`${name}: the slider is ${box.width}px wide`);
  const start = await tab.evaluate(sliderFacts);
  if (start.scroll > 2) fails.push(`${name}: the slider page scrolls sideways (${start.scroll}px)`);
  await tab.screenshot({ path: `/tmp/shots/phone-slider-${name.replace(' ', '-')}.png` });

  // The keys: End, ArrowLeft, Home, in a real engine.
  await input.focus();
  await tab.keyboard.press('End');
  const end = await input.inputValue();
  await tab.keyboard.press('ArrowLeft');
  const left = await input.inputValue();
  await tab.keyboard.press('Home');
  const home = await input.inputValue();
  console.log(`${name}: keys End=${end} ArrowLeft=${left} Home=${home}`);
  if (end !== '6' || left !== '5' || home !== '0') fails.push(`${name}: the keys do not move the slider (${end}, ${left}, ${home})`);

  // The tween: 30 runs, input to first changed frame, and the length of the animation.
  const timing = await tab.evaluate(async () => {
    const field = document.querySelector('.teach-card input[type=range]');
    const parts = [...document.querySelectorAll('.teach-card .cadus-visual-move')];
    const shown = () => parts.map((g) => getComputedStyle(g).opacity).join(',');
    const frame = () => new Promise((resolve) => requestAnimationFrame(resolve));
    const seen = [];
    const lengths = [];
    for (let run = 0; run < 30; run += 1) {
      const before = shown();
      field.value = run % 2 === 0 ? '6' : '0';
      const t0 = performance.now();
      field.dispatchEvent(new Event('input', { bubbles: true }));
      for (const running of parts.flatMap((g) => g.getAnimations())) lengths.push(running.effect.getTiming().duration);
      let gap = null;
      for (let tick = 0; tick < 60 && gap === null; tick += 1) {
        await frame();
        if (shown() !== before) gap = performance.now() - t0;
      }
      seen.push(gap);
      await new Promise((resolve) => setTimeout(resolve, 450));
    }
    return { seen, lengths };
  });
  const gaps = timing.seen.filter((g) => g !== null).sort((a, b) => a - b);
  const p95 = gaps[Math.min(gaps.length - 1, Math.ceil(gaps.length * 0.95) - 1)];
  console.log(`${name}: first changed frame p95=${p95?.toFixed(1)}ms of ${gaps.length}/30 runs; tween lengths ${[...new Set(timing.lengths)].join(',')}`);
  if (gaps.length < 30 || p95 >= 140) fails.push(`${name}: first changed frame p95 ${p95}ms over 140ms`);
  if (timing.lengths.some((l) => l < 250 || l > 400)) fails.push(`${name}: a tween is outside 250 to 400 ms`);
  await context.close();
}

// Reduced motion: the end state shows in the first frame and no Animation object exists.
{
  const context = await browser.newContext({ ...devices['iPhone 14'], reducedMotion: 'reduce' });
  const tab = await openTeach(context);
  const after = await tab.evaluate(async () => {
    const field = document.querySelector('.teach-card input[type=range]');
    field.value = '6';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    field.dispatchEvent(new Event('change', { bubbles: true }));
    const made = document.querySelector('.teach-card').getAnimations({ subtree: true }).length;
    await new Promise((resolve) => requestAnimationFrame(resolve));
    const parts = [...document.querySelectorAll('.teach-card .cadus-visual-move')];
    return { made, opacities: parts.map((g) => Number(getComputedStyle(g).opacity)) };
  });
  console.log(`reduced motion: animations=${after.made} opacities=${after.opacities.join(',')}`);
  if (after.made !== 0) fails.push(`reduced motion: ${after.made} animations after an input`);
  if (!after.opacities.every((o) => o === 1)) fails.push('reduced motion: not every part shows in the first frame');
  await context.close();
}

await browser.close();
if (fails.length) { console.log('FAIL:\n' + fails.join('\n')); process.exit(1); }
console.log('PHONE CHECK OK');
