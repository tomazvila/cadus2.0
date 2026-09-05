/**
 * The queue, grouped by knowledge point.
 *
 * Spec section 3.2 asks for "a left list of pending items grouped by KP with kind,
 * attempts, and cost". The grouping is the reviewer's unit of work: eight templates for one
 * knowledge point are read together, because the eighth is only worth approving if it is
 * not the seventh again.
 *
 * The order is TOTAL and derived from the payload alone, so two renders of one reply give
 * one order and the keyboard walk is repeatable: knowledge points by id, and inside a
 * group, newest first with the digest as the tie break. That inner order is the order
 * `cadus_store::content::review_list` already returns, restated here so a screen keyed on
 * position does not depend on a `Map` insertion order the caller could change.
 */
import type { ReviewItem } from '@/api/types';

/** One knowledge point's pending documents. */
export interface KpGroup {
  kp_id: string;
  /** True while this knowledge point holds fewer than `bank_target` approved templates. */
  bank_warning: boolean;
  /** Approved templates the knowledge point already holds. */
  approved_templates: number;
  items: ReviewItem[];
}

export function groupByKp(items: readonly ReviewItem[]): KpGroup[] {
  const byKp = new Map<string, KpGroup>();
  for (const item of items) {
    const group = byKp.get(item.kp_id) ?? {
      kp_id: item.kp_id,
      // The warning belongs to the knowledge point, and every row of one knowledge point
      // carries the same value; the first row of the group states it for the group.
      bank_warning: item.bank_warning,
      approved_templates: item.approved_templates,
      items: [],
    };
    group.items.push(item);
    byKp.set(item.kp_id, group);
  }
  const groups = [...byKp.values()];
  groups.sort((a, b) => a.kp_id.localeCompare(b.kp_id));
  for (const group of groups) {
    group.items.sort(
      (a, b) => b.created_at.localeCompare(a.created_at) || a.digest.localeCompare(b.digest),
    );
  }
  return groups;
}

/**
 * The digests in the order the screen renders them.
 *
 * `j` and `k` walk THIS list, so the keyboard and the eye move together across a group
 * boundary. A per-group walk would strand the reviewer at the last row of a group.
 */
export function walkOrder(groups: readonly KpGroup[]): string[] {
  return groups.flatMap((group) => group.items.map((item) => item.digest));
}

/**
 * The digest one step away from `digest`, or the first one when nothing is selected.
 *
 * It STOPS at both ends rather than wrapping. A wrap sends a reviewer who pressed `j` once
 * too often back to the top of a queue they have just worked through, and the row they
 * decide on next is then the row they already decided on.
 */
export function step(order: readonly string[], digest: string | null, delta: number): string | null {
  if (order.length === 0) return null;
  // Nothing selected, or a digest the reload dropped, stands before the first row: one step
  // either way lands on it.
  const at = order.findIndex((d) => d === digest);
  return order[Math.min(order.length - 1, Math.max(0, at + delta))];
}
