/**
 * Types for the vendored Cytoscape build.
 *
 * `/vendor/cytoscape/cytoscape.esm.min.mjs` is a RUNTIME url Caddy serves out of `dist/`,
 * not a package: nothing on disk resolves it at build time, and `vite.config.ts` marks
 * `/vendor/**` external so Rollup emits the specifier untouched.
 *
 * It cannot be an ambient `declare module`. A specifier that starts with `/` is an absolute
 * PATH to TypeScript, not a module name, so the declaration is never consulted. Both
 * tsconfigs therefore `paths`-map the specifier: the production build maps it here, and
 * `tsconfig.json` maps it to the double in `test/mocks/cytoscape.ts`.
 *
 * The signature is narrow on purpose, not `any`: the loader's whole job is to hand back
 * something callable, and `cytoscape-loader.ts` declares the surface the map calls.
 */
import type { CytoscapeFactory } from './views/map/cytoscape-loader';

declare const cytoscape: CytoscapeFactory;
export default cytoscape;
