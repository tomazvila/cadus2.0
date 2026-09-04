/**
 * The T3 roll-up: what one knowledge point cost to author.
 *
 * WHERE THE NUMBERS COME FROM. `GET /api/operator/flags` carries the A6 serving health and
 * no money at all; the bill lives on the stored document, as `authoring_attempts` and
 * `authoring_cost_usd` (`crates/worker/src/authoring/cost.rs`, T3). So the operator screen
 * reads the whole review queue — every status, not only `pending` — and groups it by
 * `kp_id`. A document that was rejected still cost money, and a roll-up that dropped it
 * would under-report the spend of the knowledge point that keeps failing its gate.
 *
 * THE STRING IS THE TRUTH, THE NUMBER IS FOR THE SCREEN. The column is `NUMERIC` and the
 * service sends a decimal string precisely so no float rounds the money on the way
 * (`cost.rs`, "Why the money never becomes a float"). A total has to be added up somewhere,
 * and a browser has only doubles, so this module adds them as doubles and every caller
 * labels the result as a total. A per-document figure is rendered from the string the
 * service sent, never from this sum.
 *
 * THE ALERT IS A COUNT, NOT A FLAG. T3 alerts above three attempts, and the count of
 * documents that passed the bound tells an operator whether one knowledge point is hard or
 * the whole batch is.
 */
import { num, type Numeric } from '@/lib/format';
import type { ReviewItem } from '@/api/types';

/**
 * The attempts one authoring pass gets before T3 alerts
 * (`crates/worker/src/authoring/cost.rs` `ATTEMPT_ALERT`).
 *
 * The comparison is STRICTLY GREATER: a pass that landed on attempt 3 is inside the bound,
 * and a pass that landed on attempt 4 alerts.
 */
export const ATTEMPT_ALERT = 3;

/** Whether one document's attempt count raises the T3 alert. */
export function alerts(item: ReviewItem): boolean {
  return num(item.authoring_attempts) > ATTEMPT_ALERT;
}

/** What one knowledge point spent. */
export interface KpCost {
  kp_id: string;
  /** Documents of every status. */
  documents: number;
  /** Model calls, summed over those documents. */
  attempts: number;
  /** The summed bill, in dollars. A display total — see the module note. */
  cost: number;
  /** How many of those documents passed [`ATTEMPT_ALERT`]. */
  alerting: number;
}

/**
 * Group a review queue by knowledge point, most expensive first.
 *
 * The tie break is the `kp_id`, so the order is total and two renders of one payload agree.
 */
export function costByKp(items: readonly ReviewItem[]): KpCost[] {
  const byKp = new Map<string, KpCost>();
  for (const item of items) {
    const row = byKp.get(item.kp_id) ?? {
      kp_id: item.kp_id,
      documents: 0,
      attempts: 0,
      cost: 0,
      alerting: 0,
    };
    row.documents += 1;
    row.attempts += num(item.authoring_attempts);
    row.cost += num(item.authoring_cost_usd);
    if (alerts(item)) row.alerting += 1;
    byKp.set(item.kp_id, row);
  }
  return [...byKp.values()].sort((a, b) => b.cost - a.cost || a.kp_id.localeCompare(b.kp_id));
}

/** The decimals a dollar figure carries. An authoring call costs well under a cent. */
const COST_DECIMALS = 4;

/**
 * A dollar figure for the screen.
 *
 * A null cost is NOT rendered as `$0.0000`. The service sends null when no `model_call_log`
 * row priced the run, and "we do not know" and "it was free" are different facts.
 */
export function usd(value: Numeric): string {
  if (value === null || value === undefined || value === '') return '—';
  const n = Number(value);
  return Number.isFinite(n) ? `$${n.toFixed(COST_DECIMALS)}` : '—';
}
