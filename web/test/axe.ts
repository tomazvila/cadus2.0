/**
 * The axe options every accessibility assertion in this suite uses.
 *
 * WHAT AXE IN JSDOM CAN AND CANNOT SEE, stated once so no result is over-read. It CAN see:
 * a missing form label, a button with no accessible name, a bad ARIA attribute or role,
 * heading order, a duplicate id, list structure, and a control whose state is unreadable.
 * It CANNOT see: color contrast, focus order as rendered, anything that needs layout, or
 * whether a live region announces (spec section 4.5). So pair axe with explicit focus and
 * keyboard assertions; axe alone is not an accessibility pass.
 *
 * `color-contrast` is DISABLED rather than left to report nothing. jsdom resolves no CSS
 * custom property, so every token color reads as the empty string and the rule is vacuous;
 * worse, it reaches for `HTMLCanvasElement.getContext`, which jsdom answers with a "Not
 * implemented" on `console.error` — and `test/setup.ts` fails a test on any console.error.
 * The contrast numbers of spec section 4.2 are pinned by the S4 token contract test, which
 * reads the stylesheet text instead of asking the browser.
 */
export const AXE_IN_JSDOM = {
  rules: {
    'color-contrast': { enabled: false },
  },
} as const;
