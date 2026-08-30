/**
 * The pure display helpers. They are the 1.0 originals of `static/ui.js`, byte for byte.
 *
 * Each one takes `unknown` and gives a number. That is the point: the dashboard reads a
 * frozen API contract, and one absent key renders `NaN / NaN XP today` to a learner.
 * `num(undefined)` gives 0 instead, and the screen stays honest.
 *
 * `clamp01` lives here, not beside the ring that draws with it. The ring needs the
 * geometry and the percent readout needs the same clamp; two copies drift.
 */

/** A finite number, or the fallback. `Number(null)` is 0, and 0 is finite. */
export function num(v: unknown, d = 0): number {
  const n = Number(v);
  return Number.isFinite(n) ? n : d;
}

/** Clamp to 0..1. A fraction outside it draws a ring arc longer than the circle. */
export function clamp01(v: unknown): number {
  const n = Number(v);
  return Number.isFinite(n) ? Math.max(0, Math.min(1, n)) : 0;
}

/** A whole percent, 0..100. `pct(0.185)` is 19. */
export function pct(v: unknown): number {
  return Math.round(clamp01(v) * 100);
}
