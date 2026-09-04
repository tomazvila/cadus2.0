/**
 * The two operator paths (spec section 4.1).
 *
 * WHY THESE TWO AND NOT THE WHOLE TABLE. Spec section 4.1 gives the reason 2.0 adds URL
 * routing at all: "the review screen and the operator screen are pages an operator links to
 * and reloads". Every learner-facing screen of that table is reached from another screen —
 * the dashboard starts the session, the session opens the quiz — so a view name carries
 * them, as it did in 1.0. These two are reached from a pasted link and from a refresh, and
 * nothing else in the SPA offers them: no topbar button, no dashboard card, no menu.
 *
 * THAT ABSENCE IS DELIBERATE. `users.is_admin` is outside the runtime role's column grants
 * (`docs/SCHEMA.md`), so no reply the SPA reads carries the flag, and the SPA cannot know
 * whether the signed-in account is an operator. A link rendered on a guess would either
 * offer every learner a screen that refuses them, or hide the screen from the operator it
 * belongs to. The service is the gate: the four review routes and the flags route answer
 * `403 forbidden`, and the two screens render that refusal (REVIEW-admin).
 *
 * Matching is EXACT, apart from one trailing slash. A prefix match would give `/opsfoo` and
 * `/review-notes` the operator screens, and a path this small has no reason to be fuzzy.
 */

/** The operator screen: the A6 flags and the T3 cost per knowledge point. */
const OPS_PATH = '/ops';

/** The review screen: the C6 queue of spec section 3.2. */
const REVIEW_PATH = '/review';

/** Which operator screen a path asks for. */
export type AdminRoute = 'ops' | 'review';

/**
 * The operator screen one path names, or null for every other path.
 *
 * `/ops/` and `/review/` count: a browser, a proxy, and a pasted link all add the slash,
 * and a reader who typed one more character did not ask for the dashboard. The root path
 * loses its one slash too, and the empty string names no screen either.
 */
export function adminRouteFor(pathname: string): AdminRoute | null {
  const path = pathname.endsWith('/') ? pathname.slice(0, -1) : pathname;
  if (path === OPS_PATH) return 'ops';
  if (path === REVIEW_PATH) return 'review';
  return null;
}
