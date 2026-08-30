#!/usr/bin/env node
/**
 * The invariant gate.
 *
 * Asserts that every tag marked `covered: true` in test/invariants.json is claimed by at
 * least one test whose NAME contains that tag. It replaces a coverage floor, which is blind
 * here: every invariant is a NEGATIVE ("never posts twice", "never fires after teardown"),
 * and deleting the F-37-1c test moves line coverage by zero.
 *
 * Exit 0 = every covered tag has a test. Exit 1 = a tag lost its test, a pending tag gained
 * one, or a retirement does not hold up.
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = join(root, 'test/invariants.json');

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) out.push(...walk(p));
    else if (/\.test\.(ts|tsx)$/.test(entry)) out.push(p);
  }
  return out;
}

/**
 * Collect TEST TITLES only — the string argument of describe/it/test.
 *
 * Deliberately not a whole-file text search. That version reported a tag as covered because
 * a COMMENT explained why the file was NOT about that tag. A gate a comment can satisfy is
 * not a gate: prose about an invariant would count as a test of it, which is the exact
 * false confidence this script exists to stop.
 */
export function stripComments(source) {
  // Block comments first, then line comments. Good enough for test files: the goal is that
  // PROSE about an invariant never counts as a test of it.
  return source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/(^|[^:])\/\/[^\n]*/g, '$1 ');
}

/**
 * Does a test title CLAIM this tag?
 *
 * Not `includes`. Two collisions make a plain substring search certify coverage that does
 * not exist:
 *   * Incidental text. 'course GRID10 loads' claims D10; 'STEP3 of the wizard' claims P3;
 *     'aR15xQ' claims R15.
 *   * Sibling prefixes. F-36-1 is a prefix of F-36-1b, and F-37-1 of F-37-1b / F-37-1c, so
 *     writing ONE of those tests would make three tags permanently unfailable.
 *
 * A tag therefore sits on its own boundary: no tag-ish character on either side.
 */
const TAG_CHAR = '[A-Za-z0-9\\-/]';

export function claims(title, tag) {
  const escaped = tag.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(`(?<!${TAG_CHAR})${escaped}(?!${TAG_CHAR})`).test(title);
}

export function titlesIn(source) {
  const out = [];
  // Handles: it('t'), it.only('t'), test.skip('t'), describe('t'), and the table forms
  // it.each([...])('t') / it.each`...`('t'), which put an argument list between the
  // identifier and the title.
  const re =
    /\b(?:describe|it|test)(?:\.\w+)*\s*(?:\([^()]*\)|`[^`]*`)?\s*\(\s*(['"`])((?:\\.|(?!\1)[^\\])*)\1/g;
  let m;
  while ((m = re.exec(source)) !== null) out.push(m[2]);
  return out;
}

/**
 * Sort the manifest into buckets. Pulled out of the main path so the selftest drives it
 * directly — the `retired` rules are the kind that look obviously right and are silently
 * wrong (a retired tag that still has a test used to count as coverage).
 */
export function classify(tags, titles) {
  const claimed = [];
  const missing = [];
  const pending = [];
  const retired = [];
  const unretired = [];

  for (const { tag, covered, retired: isRetired, why, what } of tags) {
    // A test claims a tag by NAMING it: it('F-37-1c: Enter during grading posts nothing').
    const hit = titles.some((t) => claims(t, tag));

    // RETIRED is not PENDING. A tag whose feature this backend cannot reach can never be
    // covered, so leaving it pending makes the manifest's own exit criterion ("every live
    // tag is true") unreachable and hides permanent zeroes among the real gaps. Retiring
    // needs a written reason, and a retired tag that acquires a test is itself an error:
    // the reason was wrong.
    if (isRetired) {
      if (!why) unretired.push({ tag, problem: 'retired with no `why`' });
      else if (hit) unretired.push({ tag, problem: 'retired, but a test names it' });
      else retired.push({ tag, why });
      continue;
    }

    if (!covered) { pending.push({ tag, what, hit }); continue; }
    if (hit) claimed.push(tag);
    else missing.push({ tag, what });
  }

  return { claimed, missing, pending, retired, unretired };
}

// --- self-test: prove a comment cannot satisfy this gate -----------------------
if (process.argv.includes('--selftest')) {
  const commentOnly = `
    // This file is NOT F-37-1c. See the session unit.
    /* F-38-1 is covered elsewhere. */
    describe('something else', () => { it('does a thing', () => {}); });
  `;
  const realTest = `
    describe('session', () => {
      it('F-37-1c: Enter during an in-flight grade posts nothing', () => {});
      it.each([1])('F-38-1: one instance per load', () => {});
    });
  `;
  const a = titlesIn(stripComments(commentOnly));
  const b = titlesIn(stripComments(realTest));
  const has = (titles, tag) => titles.some((t) => claims(t, tag));

  const fails = [];

  // 1. Prose must not count.
  if (has(a, 'F-37-1c')) fails.push('a line comment satisfied F-37-1c');
  if (has(a, 'F-38-1')) fails.push('a block comment satisfied F-38-1');

  // 2. Real titles must count, including the it.each table form.
  if (!has(b, 'F-37-1c')) fails.push('a real it() title did NOT satisfy its tag');
  if (!has(b, 'F-38-1')) fails.push('an it.each() title did NOT satisfy its tag');

  // 3. Incidental text must not count. Each of these was a REAL false positive in 1.0.
  const incidental = ['course GRID10 loads', 'STEP3 of the wizard renders', 'hash aR15xQ is stable'];
  if (has(incidental, 'D10')) fails.push("'GRID10' satisfied the D10 tag");
  if (has(incidental, 'P3')) fails.push("'STEP3' satisfied the P3 tag");
  if (has(incidental, 'R15')) fails.push("'aR15xQ' satisfied the R15 tag");

  // 4. A sibling tag must not satisfy its own prefix. Writing the F-36-1b test must leave
  //    F-36-1 unclaimed, or three tags become permanently unfailable.
  const siblings = ['F-36-1b: an actionable toast never auto-dismisses'];
  if (has(siblings, 'F-36-1')) fails.push("'F-36-1b' satisfied the shorter F-36-1 tag");
  if (!has(siblings, 'F-36-1b')) fails.push('F-36-1b did not satisfy its own tag');

  // 5. A tag with a slash still matches on its own boundary.
  if (!has(['DD-3/P1: the re-solve locks it in'], 'DD-3/P1')) fails.push('DD-3/P1 did not match');

  // 6. Retirement is a claim about the BACKEND, and it has to be defensible.
  const titles6 = ['X-live: it does the thing', 'X-zombie: still tested'];
  const c = classify([
    { tag: 'X-live', covered: true, what: 'w' },
    { tag: 'X-gone', retired: true, why: 'the route always answers 404', what: 'w' },
    { tag: 'X-zombie', retired: true, why: 'claims to be unreachable', what: 'w' },
    { tag: 'X-mute', retired: true, what: 'w' },
    { tag: 'X-todo', covered: false, what: 'w' },
    { tag: 'X-lost', covered: true, what: 'w' },
  ], titles6);

  if (c.claimed.length !== 1) fails.push('a covered tag with a test was not counted');
  if (c.retired.length !== 1 || c.retired[0].tag !== 'X-gone') {
    fails.push('a properly retired tag was not retired');
  }
  if (c.pending.length !== 1) fails.push('a pending tag was miscounted');
  if (c.missing.length !== 1) fails.push('a covered tag with NO test was not reported missing');
  // A retirement with no reason, and one contradicted by a live test, must both fail.
  const bad = c.unretired.map((u) => u.tag).sort();
  if (bad.join(',') !== 'X-mute,X-zombie') {
    fails.push(`bad retirements not caught: got [${bad.join(', ')}]`);
  }

  // 7. The manifest on disk must parse and carry tags. A gate over an empty list passes
  //    forever, and that is the failure this whole file exists to prevent.
  const onDisk = JSON.parse(readFileSync(manifestPath, 'utf8'));
  if (!Array.isArray(onDisk.tags) || onDisk.tags.length === 0) {
    fails.push('test/invariants.json carries no tags');
  }

  if (fails.length) {
    console.error('SELFTEST FAIL:');
    for (const f of fails) console.error(`  ${f}`);
    process.exit(1);
  }
  console.log('invariants selftest: PASS (prose, incidental text, sibling prefixes and bad retirements all rejected)');
  process.exit(0);
}

const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
const files = walk(join(root, 'test'));
const titles = files.flatMap((f) => titlesIn(stripComments(readFileSync(f, 'utf8'))));

const { claimed, missing, pending, retired, unretired } = classify(manifest.tags, titles);

const live = manifest.tags.length - retired.length;
console.log(
  `invariants: ${claimed.length}/${live} covered · ${pending.length} pending · `
  + `${retired.length} retired · ${files.length} test files · ${titles.length} test titles`,
);

if (retired.length) {
  console.log('\nRetired (unreachable on this backend):');
  for (const { tag, why } of retired) console.log(`  ${tag} — ${why}`);
}

// A tag still marked pending that already HAS a test is a bookkeeping slip: flip it.
const readyToFlip = pending.filter((p) => p.hit);
if (readyToFlip.length) {
  console.log('\nThese tags have tests but are still marked covered:false — flip them:');
  for (const { tag } of readyToFlip) console.log(`  ${tag}`);
}

if (missing.length) {
  console.error('\nFAIL — these tags are marked covered but no test names them:');
  for (const { tag, what } of missing) console.error(`  ${tag} — ${what}`);
  console.error('\nEither restore the test, or change its `covered` flag deliberately.');
  process.exit(1);
}

if (unretired.length) {
  console.error('\nFAIL — these retirements do not hold up:');
  for (const { tag, problem } of unretired) console.error(`  ${tag} — ${problem}`);
  process.exit(1);
}

if (readyToFlip.length) process.exit(1);
