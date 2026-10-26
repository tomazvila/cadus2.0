/**
 * Types for the vendored MathLive build.
 *
 * `/vendor/mathlive/mathlive.min.mjs` is a RUNTIME url served out of `dist/`, not a package.
 * A specifier that starts with `/` is a path to TypeScript, so an ambient `declare module`
 * does not reach it. Both tsconfigs map the specifier with `paths`: the production build
 * maps it here, and `tsconfig.json` maps it to `test/mocks/mathlive.ts`.
 *
 * The surface is narrow on purpose. `lib/math-editor-loader.ts` declares what the field calls.
 */
import type { MathfieldStatics } from './lib/math-editor-loader';

export declare const MathfieldElement: MathfieldStatics;
