/**
 * The one rule of a rejection reason (REVIEW-reason).
 *
 * 1.0 makes `--reason` a required argument of `cmd_reject`
 * (`scripts/review_templates.py:201-214`), and `crates/web/src/admin.rs` `reason_of`
 * refuses an absent, blank, or over-long reason with `422`. This is the same rule, applied
 * before the post rather than after it, so a reviewer reads why the button did nothing
 * instead of reading a status code.
 *
 * IT IS THE ONLY GUARD, ON PURPOSE. A second check beside the button would mean a broken
 * rule still blocks the post, and the test that proves the rule would stay green with the
 * rule deleted. The dialog resolves what this function returns, and the screen posts what
 * the dialog resolved.
 *
 * The trim matches the service, which trims before the write: a reason of spaces alone is
 * no reason, here and there.
 */

/**
 * The most characters a reason holds (`admin.rs` `REASON_MAX_CHARS`).
 *
 * The service counts CHARACTERS, not bytes, so this counts characters too — and both count
 * the trimmed string, so trailing spaces never push a usable reason past the bound.
 */
export const REASON_MAX_CHARS = 1000;

/** The line a blank reason earns. */
export const REASON_REQUIRED =
  'A rejection needs a reason. The next author reads it, and a refusal with no reason '
  + 'tells them nothing.';

/** The line an over-long reason earns. */
export const REASON_TOO_LONG = `A reason holds at most ${REASON_MAX_CHARS} characters.`;

/** The usable form of a typed reason, or the line that says why there is none. */
export function usableReason(raw: string): { reason: string } | { error: string } {
  const reason = raw.trim();
  if (!reason) return { error: REASON_REQUIRED };
  // `[...reason]` counts code points, which is what `chars().count()` counts in the
  // service. `reason.length` counts UTF-16 units and would refuse a shorter string of
  // emoji than the service accepts.
  if ([...reason].length > REASON_MAX_CHARS) return { error: REASON_TOO_LONG };
  return { reason };
}
