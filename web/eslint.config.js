import js from '@eslint/js';
import globals from 'globals';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import jsxA11y from 'eslint-plugin-jsx-a11y';
import vitest from '@vitest/eslint-plugin';

export default tseslint.config(
  { ignores: ['dist', 'node_modules', 'coverage'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      ecmaVersion: 2022,
      globals: { ...globals.browser, ...globals.node },
    },
    plugins: { 'react-hooks': reactHooks, 'jsx-a11y': jsxA11y },
    rules: {
      ...reactHooks.configs.recommended.rules,

      // Accessibility GATES the build (spec section 4.5). Configure a deliberate exception
      // explicitly, so the intent survives in git.
      ...jsxA11y.flatConfigs.recommended.rules,

      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/consistent-type-imports': 'error',

      // `any` defeats the point of a TypeScript rewrite (O3).
      '@typescript-eslint/no-explicit-any': 'error',
    },
  },
  {
    files: ['test/**/*.{ts,tsx}'],
    plugins: { vitest },
    rules: {
      // A test reaches into shapes the production types do not describe — a raw fetch body,
      // a stubbed global.
      '@typescript-eslint/no-explicit-any': 'off',

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
    // The gate scripts are Node ESM, not browser code. They are the most load-bearing files
    // here, and in 1.0 they were linted by nothing at all.
    files: ['scripts/**/*.mjs', '*.config.js'],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: 'module',
      globals: { ...globals.node },
    },
  },
);
