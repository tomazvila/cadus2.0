/**
 * The 2.0 HTTP contract, part 3: the operator view and the review surface.
 *
 * `types.ts` carries the conventions and the source-of-truth order. Every rule there holds
 * here.
 */
import type { JsonValue } from './types';

// ---------------------------------------------------------------------------
// The operator view (A6, T3)
// ---------------------------------------------------------------------------

/** One knowledge point's serving health (`operator.rs` `flag_json`). */
interface OperatorFlag {
  kp_id: string;
  approved_templates: number;
  pool_depth: number;
  last_source: string | null;
  last_exemplar_at: string | null;
  needs_template: boolean;
  source_exhausted: boolean;
}

/** One gate run over an approved template body, asked for with `?kp=`. */
export interface OperatorGateNote {
  kp_id: string;
  digest: string;
  gated: boolean;
  reason?: string;
  exhaustive?: boolean;
  instances_checked?: number;
  notes: string[];
  rejected?: { code: string; message: string };
}

/** `GET /api/operator/flags` — admin only; a non-admin gets `403 forbidden`. */
export interface OperatorFlagsResponse {
  flags: OperatorFlag[];
  gate: OperatorGateNote[];
  gate_limit: number;
  gate_truncated: boolean;
}

// ---------------------------------------------------------------------------
// The review surface (C6, spec section 3.2)
// ---------------------------------------------------------------------------

/** The filters `GET /api/admin/content` reads. An absent key applies no filter. */
export interface ContentFilter {
  /** `pending`, `approved`, or `rejected`. */
  status?: string;
  /** `template`, `teach`, `hint_ladder`, or `diagnosis`. */
  kind?: string;
  /** One serving key, `topic:point`. */
  kp?: string;
  /** The zero-based page (`admin.rs` `PAGE_PARAM`). An absent page is page 0. */
  page?: number;
}

/**
 * One line of the review queue (`admin.rs` `item_json`).
 *
 * `authoring_cost_usd` IS A STRING, not a number. The column is `NUMERIC`, the store reads
 * it as a decimal string, and the handler writes that string through. Parsing it to a
 * float here would round money the reviewer is asked to approve; the screen sums it as a
 * number only for a total it labels as such.
 */
export interface ReviewItem {
  digest: string;
  kp_id: string;
  kind: string;
  status: string;
  authoring_attempts: number;
  /** A decimal string, or null when no T6 row priced the run. */
  authoring_cost_usd: string | null;
  /** RFC 3339. */
  created_at: string;
  /** The first `SUMMARY_CHARS` characters of the statement (`admin.rs` `summary`). */
  summary: string;
  approved_templates: number;
  /** True while this knowledge point holds fewer than `bank_target` approved templates. */
  bank_warning: boolean;
}

/** `GET /api/admin/content` — the queue, plus the two bounds it was built under. */
export interface ReviewListResponse {
  items: ReviewItem[];
  /** `cadus_store::content::BANK_TARGET`. The screen never hard-codes 3. */
  bank_target: number;
  /** `cadus_store::content::LIST_LIMIT`. A full page means the queue is longer. */
  limit: number;
}

/** One rendered instance of a pending template: the statement and its computed answer. */
interface RenderedInstance {
  text: string;
  answer: string;
}

/**
 * `GET /api/admin/content/{digest}` — the document a reviewer decides on.
 *
 * `gate` and `instances` belong to a template. Every other kind carries `gate: null` and an
 * empty instance list, because there is no statement to render and no answer to compute.
 */
export interface ReviewDocument extends ReviewItem {
  /** Current trusted exercise policy; null for ordinary objectives. */
  policy_digest?: string | null;
  /** Policy captured by the existing approval, when present. */
  approved_policy_digest?: string | null;
  /** Current eligible template-bank fingerprint for instruction review. */
  template_context_digest?: string | null;
  /** Template bank covered by the existing instruction approval. */
  approved_template_context_digest?: string | null;
  /** Effective canonical curriculum reviewed with this document. */
  curriculum_digest: string;
  /** Curriculum captured by the existing approval, when present. */
  approved_curriculum_digest: string | null;
  /** Compiled renderer/checker identity reviewed with this document. */
  review_engine_digest: string;
  /** Engine captured by the existing approval, when present. */
  approved_review_engine_digest: string | null;
  /** Authoritative template membership, in digest order. */
  eligible_template_digests?: string[];
  /** RFC 3339, or null while the document is not approved. */
  approved_at: string | null;
  /** The reason a reviewer refused it, or null. */
  review_reason: string | null;
  /** The authored payload, verbatim. The screen renders it and rewrites nothing. */
  body: JsonValue;
  gate: OperatorGateNote | null;
  instances: RenderedInstance[];
  /** Why the instance list is empty, when it is. Never an empty list with no cause (A6). */
  instances_note: string | null;
  /** `admin.rs` `SAMPLE_INSTANCES`. */
  sample_instances: number;
}

/** `POST /api/admin/content/{digest}/approve`. */
export interface ApproveResponse {
  digest: string;
  status: string;
  approved_at: string | null;
  approved_policy_digest: string | null;
  approved_template_context_digest: string | null;
  approved_curriculum_digest: string;
  approved_review_engine_digest: string;
}

/** `POST /api/admin/content/{digest}/reject`. */
export interface RejectResponse {
  digest: string;
  status: string;
}

/** One ungraded attempt of the recovery list (D-F2). */
interface UngradedAttempt {
  attempt_id: string;
  topic: string;
  /** Why the checker reached no verdict, in the service's own words. */
  reason: string;
}

/** `GET /api/admin/ungraded`. The list is oldest first. */
export interface UngradedListResponse {
  items: UngradedAttempt[];
  /** The count of entries the recovery list keeps. */
  limit: number;
}

/** `POST /api/admin/ungraded/{attempt_id}/regrade`. */
export interface RegradeResponse {
  attempt_id: string;
  /** The verdict the human reached. */
  outcome: 'correct' | 'incorrect';
  /** The correction forced the whole-log replay (D-O6). */
  replayed: boolean;
  /** The count of ungraded attempts still waiting. */
  ungraded: number;
}


// ---------------------------------------------------------------------------
// f19-retention: `GET /api/report/retention` (D-F11, D-F12).
// ---------------------------------------------------------------------------

/**
 * The counts behind one retention rate.
 *
 * A rate is never read without them: `correct` counts every graded right answer,
 * and `independent` counts only the graded, unassisted, first-exposure ones.
 */
interface RetentionProvenance {
  independent: number;
  independent_correct: number;
  correct: number;
  assisted: number;
  repeated: number;
  unknown_exposure: number;
  ungraded: number;
}

/** One delay of the retention report. `delay_days` of 0 is the total row. */
export interface RetentionRow {
  delay_days: number;
  probes: number;
  /** `null` means no independent probe answered yet. It is never a zero. */
  retained_accuracy: number | null;
  assistance_dependence: number | null;
  mean_independent_secs: number | null;
  /** Whether the independent sample reaches `min_sample`. */
  sufficient: boolean;
  provenance: RetentionProvenance;
}

/** The versioned policy the numbers came from (D-F12). */
interface RetentionPolicy {
  version: number;
  label: string;
  calibrated: boolean;
  digest: string;
  probe_delays_days: number[];
  min_sample: number;
}

/** `GET /api/report/retention`. */
export interface RetentionReportResponse {
  policy: RetentionPolicy;
  retention: {
    by_delay: RetentionRow[];
    total: RetentionRow;
  };
  placement: {
    failed_confirmation: string[];
    awaiting_confirmation: string[];
  };
  integrated: {
    served: number;
    passed: number;
    failed: number;
    inconclusive: number;
    open: number;
    pass_rate: number | null;
  };
}
