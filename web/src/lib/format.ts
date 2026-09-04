/**
 * The pure display helpers. They are the 1.0 originals of `static/ui.js`, byte for byte.
 *
 * Each one takes a `Numeric` and gives a number. That is the point: the dashboard reads a
 * frozen API contract, and one absent key renders `NaN / NaN XP today` to a learner.
 * `num(undefined)` gives 0 instead, and the screen stays honest.
 *
 * `clamp01` lives here, not beside the ring that draws with it. The ring needs the
 * geometry and the percent readout needs the same clamp; two copies drift.
 */

/**
 * What the contract carries where a screen wants a number: a number, a decimal string
 * (`authoring_cost_usd`), or the null and the absence the handlers write.
 */
export type Numeric = number | string | null | undefined;

/** A finite number, or the fallback. `Number(null)` is 0, and 0 is finite. */
export function num(v: Numeric, d = 0): number {
  const n = Number(v);
  return Number.isFinite(n) ? n : d;
}

/** Clamp to 0..1. A fraction outside it draws a ring arc longer than the circle. */
export function clamp01(v: Numeric): number {
  const n = Number(v);
  return Number.isFinite(n) ? Math.max(0, Math.min(1, n)) : 0;
}

/** A whole percent, 0..100. `pct(0.185)` is 19. */
export function pct(v: Numeric): number {
  return Math.round(clamp01(v) * 100);
}

/** Sign-prefixed, for an XP readout: '+5', '-4', '+0' — never '+-4'. */
export function signed(v: Numeric, d = 0): string {
  const n = num(v, d);
  return (n >= 0 ? '+' : '') + n;
}

/**
 * `m:SS`, floored, never negative. No hours component, by design.
 *
 * NOT byte-identical to 1.0, and the difference is deliberate. Vanilla writes `secs || 0`,
 * which passes a non-numeric value straight through: `fmtClock('nope')` paints `NaN:NaN`
 * and `fmtClock(Infinity)` paints `Infinity:NaN` — in a timer, on screen, once a second.
 * The route through `num()` paints `0:00` instead. Every numeric input agrees with vanilla.
 */
export function fmtClock(secs: Numeric): string {
  const total = Math.max(0, Math.floor(num(secs)));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
}
