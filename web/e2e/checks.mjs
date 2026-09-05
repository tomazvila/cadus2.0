/**
 * What every click-through records, and the three failures it exists to catch.
 *
 * THE THREE FAILURES ARE NAMED HERE, ONCE. `demo.mjs` calls them, `breaks.mjs` names the
 * break that reproduces each, and `run.mjs` matches the two. A detector written inline in
 * the walk could not be paired with its break, and the acceptance check of S13 is exactly
 * that pairing.
 *
 * The 1.0 click-through found all three in one session, and every one of the 710 jsdom
 * tests and every HTTP-level check passed them straight through
 * (`/home/deploy/dev/cadus/web/e2e/README.md`).
 */

/** The literal each failure reports. `run.mjs` greps the run output for these. */
export const FAIL_BLANK = 'FAIL-1 blank page';
export const FAIL_LATEX = 'FAIL-2 raw LaTeX';
export const FAIL_SERVE = 'FAIL-3 wrong problem';

/** A raw TeX control sequence, which is what an unrendered problem shows. */
const RAW_TEX = /\\(frac|dfrac|sqrt|times)/;

/**
 * One run: the page, its recorder, its screenshots, and its step log.
 *
 * Every console error, page error, failed request and 4xx/5xx lands in `problems`, and a
 * non-empty `problems` exits the script non-zero. That blanket rule is what caught two of
 * the three 1.0 failures before anyone knew what to look for.
 */
export class Run {
  /**
   * @param {import('playwright').Page} page
   * @param {{ shots: string, prefix?: string, ignore?: RegExp[] }} opts
   */
  constructor(page, opts) {
    this.page = page;
    this.shots = opts.shots;
    this.prefix = opts.prefix ?? '';
    this.problems = [];
    this.steps = [];
    this.shotCount = 0;
    const ignore = opts.ignore ?? [];

    const record = (line) => {
      if (ignore.some((re) => re.test(line))) return;
      this.problems.push(line);
    };

    page.on('console', (m) => {
      if (m.type() === 'error') record(`console.error: ${m.text().slice(0, 220)}`);
    });
    page.on('pageerror', (e) => { record(`pageerror: ${String(e).slice(0, 220)}`); });
    page.on('requestfailed', (r) => {
      record(`requestfailed: ${r.url().slice(0, 120)} — ${r.failure()?.errorText ?? '?'}`);
    });
    page.on('response', (r) => {
      if (r.status() >= 400) record(`HTTP ${r.status()}: ${r.url().slice(0, 120)}`);
    });
  }

  /** Log one completed step. The log is the artifact a reader checks the walk against. */
  note(line) {
    this.steps.push(line);
    console.log(`  ${line}`);
  }

  /** Record a failure. The run still walks on, so one screen never hides the next. */
  fail(line) {
    this.problems.push(line);
    console.log(`  ! ${line}`);
  }

  /**
   * A screenshot, numbered in walk order. These are the CI artifacts.
   *
   * A failed screenshot is NEVER the verdict. The one the walk wants most is the one taken
   * after a throw, and a page that is hung is exactly the page a screenshot times out on;
   * an escaping error there would replace the real failure with a screenshot failure.
   */
  async snap(name) {
    this.shotCount += 1;
    const n = String(this.shotCount).padStart(2, '0');
    try {
      await this.page.screenshot({
        path: `${this.shots}/${this.prefix}${n}-${name}.png`,
        timeout: 10000,
      });
    } catch (e) {
      console.log(`  (no screenshot ${name}: ${String(e).split('\n')[0].slice(0, 100)})`);
    }
  }

  count(selector) {
    return this.page.locator(selector).count();
  }

  text(selector) {
    return this.page.locator(selector).first().innerText();
  }

  /** The exit code, and the report that explains it. */
  report() {
    console.log(`\n=== ${this.steps.length} steps completed ===`);
    if (this.problems.length === 0) {
      console.log('NO console errors, page errors, failed requests or 4xx/5xx.');
      return 0;
    }
    console.log(`PROBLEMS (${this.problems.length}):`);
    for (const p of [...new Set(this.problems)]) console.log(`  ! ${p}`);
    return 1;
  }
}

/**
 * FAILURE 1 — the blank page.
 *
 * Vite rewrote `<link rel="stylesheet" href="/vendor/katex/katex.min.css">` into
 * `import "/vendor/katex/katex.min.css"` at the top of the entry chunk, because
 * `/vendor/**` is external. Chrome refuses a stylesheet as a module script, so the entry
 * NEVER EVALUATED. Every asset answered 200 and the page was white.
 *
 * The detector is deliberately not "did the dashboard render": a slow reply would report
 * the same thing. It asks whether the mount point holds any element at all after the wait,
 * because a document whose entry never ran leaves `#view` exactly as `index.html` shipped
 * it — empty.
 *
 * @param {Run} run
 */
export async function checkNotBlank(run, selector, timeout = 20000) {
  try {
    await run.page.waitForSelector(selector, { timeout });
    return true;
  } catch {
    const mounted = await run.page.evaluate(
      () => document.getElementById('view')?.childElementCount ?? -1,
    );
    run.fail(
      `${FAIL_BLANK}: ${selector} never rendered; #view holds ${mounted} elements. `
      + 'The entry chunk did not evaluate — check that no JS chunk imports a stylesheet.',
    );
    return false;
  }
}

/**
 * FAILURE 2 — every problem printed as raw LaTeX.
 *
 * `auto-render.min.js` was injected AHEAD of `katex.min.js`. The extension reads
 * `window.katex` at load (`e.renderMathInElement=t(e.katex)` in its UMD header), so it
 * captured `undefined` and threw on every render. `lib/katex.ts` catches that and falls
 * back to escaped plain text — the documented graceful degradation — so the ONLY symptom
 * was `$\dfrac{1}{2}$` on screen. jsdom stubs the global entirely and cannot see the order.
 *
 * Two clauses, because either alone is weak: KaTeX must have produced nodes, AND no raw
 * control sequence may be visible.
 *
 * @param {Run} run
 */
export async function checkMathRendered(run, selector, where) {
  const nodes = await run.count(`${selector} .katex`);
  const shown = await run.text(selector);
  if (nodes === 0) {
    run.fail(
      `${FAIL_LATEX}: ${where} rendered 0 KaTeX nodes. `
      + 'katex.min.js must load BEFORE auto-render.min.js.',
    );
    return false;
  }
  if (RAW_TEX.test(shown)) {
    run.fail(`${FAIL_LATEX}: ${where} still shows raw TeX: ${shown.slice(0, 80)}`);
    return false;
  }
  return true;
}

/**
 * FAILURE 3 — the demo lesson practised the wrong problem.
 *
 * `POST /task/{id}/serve` is IDEMPOTENT on the real server and re-stamps `started_at`. The
 * 1.0 demo backend advanced its cursor on every call, so the lesson's warm-up consumed
 * problem 1 and the learner practised problem 2 while reading the worked example for
 * problem 1. "No unit test makes two serve calls in a row."
 *
 * This is the browser check that does: leave the lesson and come back. Two visits are two
 * serves with no answer between them, so the second must repeat the first — the same
 * position AND the same statement.
 *
 * @param {Run} run
 */
export function checkSameProblem(run, first, second) {
  if (first.position === second.position && first.text === second.text) return true;
  run.fail(
    `${FAIL_SERVE}: re-entering the lesson served a different problem — `
    + `${first.position} "${first.text.slice(0, 40)}" became `
    + `${second.position} "${second.text.slice(0, 40)}". `
    + '/serve must be idempotent until an answer commits it (trap T13).',
  );
  return false;
}

/**
 * Run one walk to its end: the report is the exit code, and a throw is one more problem.
 *
 * The whole call log goes into the report, not its first line: Playwright's retry reason —
 * the element it waited for, and what intercepted the pointer — lives in the lines after it.
 *
 * @param {() => Promise<void>} main
 * @param {Run} run
 * @param {import('playwright').Browser} browser
 */
export async function finishWalk(main, run, browser) {
  try {
    await main();
  } catch (e) {
    run.fail(`THREW: ${String(e).split('\n').slice(0, 12).join(' | ').slice(0, 700)}`);
    await run.snap('failure');
  }
  await browser.close();
  process.exit(run.report());
}
