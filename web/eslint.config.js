import js from '@eslint/js';
import globals from 'globals';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import jsxA11y from 'eslint-plugin-jsx-a11y';
import vitest from '@vitest/eslint-plugin';
import sonarjs from 'eslint-plugin-sonarjs';

export default tseslint.config(
  // `e2e/work` holds the deliberately broken copies of this tree, and `e2e/shots` the
  // screenshots. Both are build output and neither is source.
  { ignores: ['dist', 'node_modules', 'coverage', 'e2e/work', 'e2e/shots'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      ecmaVersion: 2022,
      globals: { ...globals.browser, ...globals.node },
    },
    plugins: { 'react-hooks': reactHooks, 'jsx-a11y': jsxA11y, sonarjs },
    rules: {
      ...reactHooks.configs.recommended.rules,

      // The quality gate (`scripts/quality.sh`): every function stays below a cyclomatic
      // complexity of 22 and a cognitive complexity of 22.
      complexity: ['error', { max: 21 }],
      'sonarjs/cognitive-complexity': ['error', 21],

      // Accessibility GATES the build (spec section 4.5). Configure a deliberate exception
      // explicitly, so the intent survives in git.
      ...jsxA11y.flatConfigs.recommended.rules,

      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/consistent-type-imports': 'error',

      // `any` defeats the point of a TypeScript rewrite (O3), and the quality gate
      // refuses `unknown` as well: a value has a type, or the code that reads it
      // narrows from a typed shape. Tests and declaration files follow the same rule.
      '@typescript-eslint/no-explicit-any': 'error',
      'no-restricted-syntax': [
        'error',
        { selector: 'TSAnyKeyword', message: 'Write the type. `any` is refused.' },
        { selector: 'TSUnknownKeyword', message: 'Write the type. `unknown` is refused.' },
      ],
    },
  },
  {
    files: ['test/**/*.{ts,tsx}'],
    plugins: { vitest },
    rules: {
      // An assertion-free test is worse than no test: it reports green forever.
      'vitest/expect-expect': 'error',
      'vitest/no-disabled-tests': 'error',
      'vitest/no-focused-tests': 'error',
      'vitest/no-identical-title': 'error',
      'vitest/no-standalone-expect': 'error',
      'vitest/valid-expect': 'error',
    },
  },
  {
    // The gate scripts and the click-through are Node ESM, not browser code. They are the
    // most load-bearing files here, and in 1.0 they were linted by nothing at all.
    files: ['scripts/**/*.mjs', '*.config.js'],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: 'module',
      globals: { ...globals.node },
    },
  },
  {
    // The click-through is Node ESM that also carries BROWSER code: the body of every
    // `page.evaluate()` callback is serialized and run inside Chromium, where `document` and
    // `localStorage` are the real globals. Both sets belong here, and only here — the gate
    // scripts above stay node-only, so a stray `document` in one of them is still an error.
    files: ['e2e/**/*.mjs'],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: 'module',
      globals: { ...globals.node, ...globals.browser },
    },
  },
);
