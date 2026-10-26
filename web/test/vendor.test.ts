/**
 * The vendored MathLive tree and the three configs that map its specifier.
 *
 * These assertions read the FILES. The tests of the field run on a double, so only a file
 * read proves that the library, its fonts and the audit record are in the tree.
 */
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

const WEB = join(dirname(fileURLToPath(import.meta.url)), '..');
const DIR = join(WEB, 'public/vendor/mathlive');
const SPECIFIER = '/vendor/mathlive/mathlive.min.mjs';

describe('public/vendor/mathlive', () => {
  it('holds the library, its licence and woff2 fonts', () => {
    expect(existsSync(join(DIR, 'mathlive.min.mjs'))).toBe(true);
    expect(existsSync(join(DIR, 'LICENSE'))).toBe(true);
    const fonts = readdirSync(join(DIR, 'fonts'));
    expect(fonts.length).toBeGreaterThan(0);
    for (const font of fonts) expect(font).toMatch(/\.woff2$/);
  });

  it('records the version and a sha256 that matches the file', () => {
    const readme = readFileSync(join(DIR, 'README'), 'utf8');
    expect(readme).toContain('0.111.0');
    const recorded = /sha256-([A-Za-z0-9+/=]+)/.exec(readme);
    expect(recorded).not.toBeNull();
    const actual = createHash('sha256').update(readFileSync(join(DIR, 'mathlive.min.mjs'))).digest('base64');
    expect(recorded![1]).toBe(actual);
  });

  it('has no token that the content security policy blocks', () => {
    const text = readFileSync(join(DIR, 'mathlive.min.mjs'), 'utf8');
    expect(text).not.toMatch(/\beval\s*\(/);
    expect(text).not.toMatch(/new\s+Function\s*\(/);
    expect(text).not.toMatch(/new\s+(Shared)?Worker\s*\(/);
    expect(text).not.toMatch(/\bimportScripts\s*\(/);
    expect(text).not.toMatch(/\bdata:[a-zA-Z][\w.+-]*\/[\w.+-]+[;,]/);
  });
});

describe('the specifier', () => {
  const read = (file: string) => readFileSync(join(WEB, file), 'utf8');

  it('maps to the double in the Vitest alias and in tsconfig.json', () => {
    expect(read('vitest.config.ts')).toContain(`'${SPECIFIER}': here('./test/mocks/mathlive.ts')`);
    expect(read('tsconfig.json')).toContain(`"${SPECIFIER}": ["./test/mocks/mathlive.ts"]`);
  });

  it('maps to the type file in tsconfig.build.json', () => {
    expect(read('tsconfig.build.json')).toContain(`"${SPECIFIER}": ["./src/vendor-mathlive.d.ts"]`);
  });

  it('stays out of package.json and stays external in the build', () => {
    const pkg = read('package.json');
    expect(pkg).not.toMatch(/"mathlive"/);
    expect(pkg).not.toMatch(/compute-engine/);
    expect(read('vite.config.ts')).toContain('/^\\/vendor\\//');
  });

  it('is the constant of the loader', () => {
    expect(read('src/lib/math-editor-loader.ts')).toContain(`MATHLIVE_URL = '${SPECIFIER}'`);
  });
});
