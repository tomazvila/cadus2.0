/**
 * The entry Vite loads, and nothing else.
 *
 * The boot itself lives in `main.tsx`, which the suite drives against its own DOM. This
 * file only starts it: a module that boots at import time cannot be imported by a test
 * without booting, so the two are kept apart. `started` is the boot promise, for a test
 * that drives this file too.
 */
import { boot } from './main';

export const started = boot();
