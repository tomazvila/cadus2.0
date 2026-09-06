/**
 * The integrated task of D-F10: one scenario, its steps, and its one grade.
 *
 * The three payloads mirror `crates/web/src/integrated/`. Two rules of that module show
 * in these types, and a screen cannot break either one:
 *
 *  * No answer is in the served view. There is no `answer` field, no `accept_also`, no
 *    `correct` flag on a method option, and no interpretation. A field the type does not
 *    name is a field no screen can render (Hard Rule 1).
 *  * A verdict and a reasoning note are DIFFERENT things. `IntegratedGrade` carries the
 *    verdicts; `reasoning` carries the learner's own words with `graded: false`. The
 *    service never marks prose right or wrong, and the screen says so.
 */

/** One quantity or constraint the scenario states. */
interface IntegratedGiven {
  label: string;
  value: string;
  /** An assumption that changes feasibility, when the author states one. */
  note?: string | null;
}

/** One question of the task, without its answer. */
interface IntegratedAsk {
  prompt: string;
  /** The unit the learner writes beside the number. */
  unit: string | null;
  /** How many hints the ladder holds. The text stays on the server. */
  hints_available: number;
}

/** One intermediate step, without its answer. */
interface IntegratedStepView {
  id: string;
  ask: IntegratedAsk;
}

/** The method choice, without the flag that says which method works. */
interface IntegratedMethodView {
  prompt: string;
  options: { id: string; label: string }[];
}

/** `POST /api/task/{task_id}/integrated` — the whole task as ONE problem. */
export interface IntegratedProblem {
  item_id: string;
  item_digest: string;
  /** Persisted opened-rung counts; absent on older service versions. */
  hints_used?: Record<string, number>;
  title: string;
  topic: string;
  domain: string;
  scenario: string;
  given: IntegratedGiven[];
  method: IntegratedMethodView | null;
  steps: IntegratedStepView[];
  final_ask: IntegratedAsk;
  /** The knowledge points the task exercises, as `<topic>/<kp>` keys. */
  skills: string[];
}

/** `POST /api/task/{task_id}/integrated/hint` — ONE rung of one ladder. */
export interface IntegratedHintResponse {
  field: string;
  index: number;
  /** `null` when the ladder has no rung at that index. A hint is never the answer. */
  hint: string | null;
  hints_available: number;
  hints_used: number;
}

/** One answered field of a submission. */
interface IntegratedFieldAnswer {
  id: string;
  answer: string;
  hints_used: number;
}

/** What the learner sends for the whole task. */
export interface IntegratedSubmission {
  method?: string | null;
  steps: IntegratedFieldAnswer[];
  final_answer: IntegratedFieldAnswer;
  /** The learner's own words. Recorded, shown back, never graded. */
  reasoning?: string;
}

/**
 * The two calls the integrated screen makes.
 *
 * The screen takes THIS, not the whole `ApiClient`: it reads no other route, and a test
 * then stubs two methods instead of forty. `ApiClient` satisfies it structurally, so a
 * caller passes the live client with no cast.
 */
export interface IntegratedApi {
  taskIntegratedHint(
    taskId: string,
    body: { field: string; index: number },
  ): Promise<IntegratedHintResponse>;
  taskIntegratedAnswer(taskId: string, body: IntegratedSubmission): Promise<IntegratedGrade>;
}

/** The verdict on one field. */
export interface IntegratedFieldGrade {
  id: string;
  answered: boolean;
  correct: boolean;
  /** True when the checker reached no verdict (D-F2). No credit follows. */
  ungraded: boolean;
  notation: boolean;
  assisted: boolean;
  skills: string[];
}

/** `POST /api/task/{task_id}/integrated/answer` — the grade of the whole task. */
export interface IntegratedGrade {
  item_id: string;
  item_digest: string;
  method: { chosen: string | null; correct: boolean; why: string | null } | null;
  steps: IntegratedFieldGrade[];
  final: IntegratedFieldGrade;
  correct_steps: number;
  total_steps: number;
  solved: boolean;
  assisted: boolean;
  ungraded: boolean;
  skills_credited: string[];
  /** What the final number means. It arrives with the grade, never before it. */
  interpretation: string;
  /** The prose the learner wrote. `graded` is always false. */
  reasoning: { recorded: boolean; graded: boolean; note: string | null };
  /**
   * True when this submission wrote the log row. A second submission of the same item
   * answers `false`: the first one stands, and no credit is given twice.
   */
  recorded: boolean;
}
