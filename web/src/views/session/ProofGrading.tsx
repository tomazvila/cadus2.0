/**
 * The background grading of a written proof (Amendment K point 6).
 *
 * The grade reply of an ungraded written proof carries `proof_grading: {id, status:
 * 'pending'}`. This file polls that job and paints its result inside the feedback panel:
 * "Checking your proof…" while the worker runs, then Pass or Needs revision with the
 * grader's feedback and its yes/no checks, each with the learner's own words as evidence.
 *
 * The poll never blocks the learner on a review: Continue stays live the whole time, and
 * the timers go through the view `Lifetime`, so they die with the view. A written proof
 * inside a LESSON blocks its knowledge point instead and has its own screen
 * (`LessonProof.tsx`, D-PR1). A failed poll is swallowed (the
 * next one reads the row again), as in the diagnosis poll.
 *
 * A quiz answer never carries the field: the service enqueues no grading while a quiz is
 * open, so nothing here can reveal a verdict before the batch reveal.
 */
import { useEffect, useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import type { ApiClient, ProofCheck, ProofGradingField, ProofGradingPoll } from '@/api/types';
import { useLifetime, type Lifetime } from '@/hooks/useLifetime';

/** The poll interval, in milliseconds. A grading takes tens of seconds to minutes. */
export const PROOF_POLL_MS = 3000;

/** Past this, the panel says the check is slow and polls at the slow interval. */
export const PROOF_DEADLINE_MS = 10 * 60_000;

/** The poll interval after the deadline. The panel never stops on its own: the job id
 * stays in the reply (and in the quiz reveal), so the result shows whenever it lands. */
export const PROOF_SLOW_POLL_MS = 30_000;

/** What the panel paints for one written proof. */
export type ProofState =
  | { status: 'pending' }
  | { status: 'slow' }
  | { status: 'failed' }
  | { status: 'capped' }
  | { status: 'pass' | 'needs_revision'; feedback: string; checks: ProofCheck[]; solution?: string };

/** The learner-facing lines of each state. */
export const PROOF_TEXT = {
  pending:
    'Your proof is saved. A grader is checking it now, and the result appears here when it lands. You can continue meanwhile.',
  slow: 'This check is taking longer than usual. Your proof is saved and will still be graded.',
  failed: 'The automatic check could not grade this proof. It stays saved for a human review.',
  capped:
    "Today's limit of automatic proof checks is reached. This proof stays saved for a human review.",
  pass: 'This proof counts toward your progress.',
  needs_revision:
    'Revise the proof using the feedback below: open it from Your proofs on the dashboard. The solution opens once a revision passes.',
} as const;

const PENDING: ProofState = Object.freeze({ status: 'pending' });

/** Read one poll answer into a panel state. */
export function proofStateOf(job: ProofGradingPoll): ProofState {
  switch (job.status) {
    case 'pass':
    case 'needs_revision': {
      const state: ProofState = {
        status: job.status,
        feedback: job.feedback ?? '',
        checks: job.checks ?? [],
      };
      return job.solution ? { ...state, solution: job.solution } : state;
    }
    case 'capped':
      return { status: 'capped' };
    case 'failed':
      return { status: 'failed' };
    default:
      return PENDING;
  }
}

/**
 * Follow the `proof_grading` field of one grade reply.
 *
 * Null when the reply carries no pending grading, so the panel stays the plain "Not marked".
 */
export function useProofGrading(
  api: ApiClient,
  life: Lifetime,
  field: ProofGradingField | undefined,
): ProofState | null {
  const id = field?.status === 'pending' ? field.id : null;
  const [landed, setLanded] = useState<{ id: string; state: ProofState } | null>(null);

  useEffect(() => {
    if (id === null) return undefined;
    let done = false;
    let interval = 0;
    let deadline = 0;
    const finish = (state: ProofState) => {
      done = true;
      life.clearTimer(interval);
      life.clearTimer(deadline);
      setLanded({ id, state });
    };
    const poll = async () => {
      let job: ProofGradingPoll;
      try {
        job = await api.getProofGrading(id);
      } catch {
        return;
      }
      if (done || !life.alive()) return;
      const state = proofStateOf(job);
      if (state.status !== 'pending') finish(state);
    };
    void poll();
    interval = life.setInterval(() => { void poll(); }, PROOF_POLL_MS);
    deadline = life.setTimeout(() => {
      if (done) return;
      // Slow, not over: say so, and keep reading the row at the slow interval.
      setLanded({ id, state: { status: 'slow' } });
      life.clearTimer(interval);
      interval = life.setInterval(() => { void poll(); }, PROOF_SLOW_POLL_MS);
    }, PROOF_DEADLINE_MS);
    return () => {
      done = true;
      life.clearTimer(interval);
      life.clearTimer(deadline);
    };
  }, [api, life, id]);

  if (id === null) return null;
  return landed?.id === id ? landed.state : PENDING;
}

/**
 * Record that the learner saw a landed verdict (`seen_at`, D-PR1). A failed write is
 * swallowed: the stamp feeds the "new verdict" marks, never the verdict itself.
 */
export function useSeen(api: ApiClient, field: ProofGradingField | undefined, state: ProofState | null): void {
  const id = field?.id ?? null;
  const landed = state !== null && state.status !== 'pending' && state.status !== 'slow';
  useEffect(() => {
    if (id === null || !landed) return;
    api.proofSeen(id).catch(() => undefined);
  }, [api, id, landed]);
}

/** The heading of a written proof, by the state of its background grading. */
export const PROOF_TITLE: Record<ProofState['status'], string> = {
  pending: 'Checking your proof…',
  slow: 'Checking your proof…',
  pass: 'Proof accepted',
  needs_revision: 'Needs revision',
  failed: 'Not marked',
  capped: 'Not marked',
};

/** The grader's verdict, feedback, checks and the reference solution. */
export function ProofResult({ state }: { state: ProofState }) {
  if (state.status !== 'pass' && state.status !== 'needs_revision') {
    return <p className="feedback-reason muted" role="status">{PROOF_TEXT[state.status]}</p>;
  }
  return (
    <div className="proof-result" data-status={state.status}>
      <p className="feedback-reason" role="status">{PROOF_TEXT[state.status]}</p>
      {state.feedback ? <MathBlock className="proof-feedback">{state.feedback}</MathBlock> : null}
      {state.checks.length ? (
        <>
          <div className="solution-label">Checks</div>
          <ul className="proof-checks">
            {state.checks.map((check) => (
              <li key={check.id} className={check.met ? 'proof-check-met' : 'proof-check-unmet'}>
                <span className="proof-check-answer">{check.met ? 'Yes' : 'No'}</span>{' '}
                <span className="proof-check-text">{check.text}</span>
                {check.minor && !check.met ? <span className="muted"> (minor)</span> : null}
                <div className="proof-check-evidence muted">
                  {check.evidence === 'not found' ? 'Not found in your proof.' : <q>{check.evidence}</q>}
                  {check.evidence !== 'not found' && check.quote_verified === false
                    ? <span className="proof-check-unquoted"> (this quote is not in your proof)</span> : null}
                </div>
              </li>
            ))}
          </ul>
        </>
      ) : null}
      {state.solution ? (
        <div className="solution">
          <div className="solution-label">Solution</div>
          <MathBlock className="solution-text">{state.solution}</MathBlock>
        </div>
      ) : null}
    </div>
  );
}

/**
 * The grading of one quiz proof, after the quiz reveal. The reveal names the job; this
 * panel follows it like the feedback panel does.
 */
export function QuizProofGrading({ api, field }: { api: ApiClient; field: ProofGradingField }) {
  const life = useLifetime();
  const state = useProofGrading(api, life, field);
  useSeen(api, field, state);
  if (!state) return null;
  return (
    <div className="quiz-proof-grading" aria-live="polite">
      <p className="feedback-title">{PROOF_TITLE[state.status]}</p>
      <ProofResult state={state} />
    </div>
  );
}
