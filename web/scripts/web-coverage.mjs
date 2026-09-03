// Fail when web coverage is below 100% or a function has a CRAP score of 25 or more.
//
// Usage: node scripts/web-coverage.mjs <coverage-final.json> [crapLimit]
//
// CRAP(f) = cc(f)^2 * (1 - cov(f))^3 + cc(f). cc comes from the ESLint `complexity` rule
// run at a limit of 0, so every function reports its own value; cov is the covered share
// of the statements inside f, from the Istanbul report that Vitest wrote.
import { readFileSync } from 'node:fs';
import { relative } from 'node:path';
import { ESLint } from 'eslint';

const [reportPath, limitArg] = process.argv.slice(2);
const LIMIT = Number(limitArg ?? 25);
const report = JSON.parse(readFileSync(reportPath, 'utf8'));

async function complexityByFile(files) {
  const eslint = new ESLint({ overrideConfig: { rules: { complexity: ['warn', 0] } } });
  const out = new Map();
  for (const result of await eslint.lintFiles(files)) {
    const perLine = new Map();
    for (const m of result.messages) {
      if (m.ruleId !== 'complexity') continue;
      const cc = Number(/complexity of (\d+)/.exec(m.message)?.[1] ?? 1);
      perLine.set(m.line, Math.max(perLine.get(m.line) ?? 0, cc));
    }
    out.set(result.filePath, perLine);
  }
  return out;
}

function inside(loc, fn) {
  const a = fn.loc.start, b = fn.loc.end;
  const afterStart = loc.start.line > a.line || (loc.start.line === a.line && loc.start.column >= a.column);
  const beforeEnd = loc.end.line < b.line || (loc.end.line === b.line && loc.end.column <= b.column);
  return afterStart && beforeEnd;
}

const files = Object.keys(report);
const complexity = await complexityByFile(files);
let gaps = 0, crapCount = 0;
for (const file of files) {
  const f = report[file];
  const short = relative(process.cwd(), file);
  const statements = Object.keys(f.s).length, hitS = Object.values(f.s).filter((n) => n > 0).length;
  const fns = Object.keys(f.f).length, hitF = Object.values(f.f).filter((n) => n > 0).length;
  const branches = Object.values(f.b).flat().length, hitB = Object.values(f.b).flat().filter((n) => n > 0).length;
  if (hitS < statements || hitF < fns || hitB < branches) {
    gaps++;
    console.log(`${short}: statements ${hitS}/${statements} functions ${hitF}/${fns} branches ${hitB}/${branches}`);
  }
  const perLine = complexity.get(file) ?? new Map();
  for (const [id, fn] of Object.entries(f.fnMap)) {
    const own = Object.entries(f.statementMap).filter(([, loc]) => inside(loc, fn));
    const cov = own.length ? own.filter(([sid]) => f.s[sid] > 0).length / own.length : (f.f[id] > 0 ? 1 : 0);
    const cc = perLine.get(fn.loc.start.line) ?? 1;
    const crap = cc * cc * (1 - cov) ** 3 + cc;
    if (crap >= LIMIT) { crapCount++; console.log(`${short}:${fn.loc.start.line} ${fn.name} cc=${cc} cov=${cov.toFixed(2)} crap=${crap.toFixed(1)}`); }
  }
}
console.log(`web coverage: ${files.length} files, ${gaps} below 100%; ${crapCount} functions with CRAP >= ${LIMIT}`);
process.exit(gaps || crapCount ? 1 : 0);
