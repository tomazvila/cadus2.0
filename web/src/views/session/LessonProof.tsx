/**
 * A written proof inside a lesson (D-PR1): write, wait for the verdict, revise, and only
 * then move on.
 *
 * The knowledge point closes on a PASS, not on the submission. The screen walks the chain
 * the service keeps:
 *
 *   write    the draft, or the revision prefilled with the last draft
 *   grading  the obligations checklist while the grader runs; after 90 s the learner may
 *            leave, and the revision stays an open item that the plan serves first
 *   verdict  pass → Continue closes the point; needs revision → the feedback and the first
 *            unmet check with its quote, and the textarea again
 *   reveal   at the cap (two revisions used) the solution shows ONCE; the learner then
 *            rewrites the proof with it hidden, which closes the point as assisted
 *
 * The reference solution reaches this screen only after a pass or in the one reveal: the
 * service never puts it in a serve or a grade reply of a lesson proof.
 */
import { useEffect, useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import { MathVisuals } from '@/components/MathVisual';
import type { Call } from '@/hooks/useCall';
import type { Lifetime } from '@/hooks/useLifetime';
import type {
  AnswerResponse,
  ApiClient,
  ProofChain,
  ProofCheck,
  ProofGradingPoll,
  ProofUnmet,
  ServedProblem,
} from '@/api/types';
import { useProofGrading } from './ProofGrading';

/** The obligations every written proof meets. Shown while writing and while grading. */
export const PROOF_OBLIGATIONS = [
  'State the claim and its hypotheses.',
  'Justify every step: name the definition, rule or earlier result it uses.',
  'Reach the stated conclusion, and say that you did.',
] as const;

/** After this long the grading screen offers to leave the proof for later. */
export const PROOF_LEAVE_AFTER_MS = 90_000;

/** The learner-facing lines of the loop. */
export const LESSON_PROOF_TEXT = {
  grading: 'A grader is checking your proof. Meanwhile, check it against these obligations:',
  leave: 'This check is taking a while. Your proof is saved: continue with the next task, and this revision comes first in your plan.',
  pass: 'Proof accepted. Continue to close this part of the lesson.',
  revise: 'Not yet. Fix the first unmet check below, then resubmit the proof.',
  reveal: 'You used both revisions. Read the solution once. Then rewrite the proof without it.',
  rewrite: 'Rewrite the proof from memory, without the solution.',
  unavailable: 'The automatic check could not grade this draft. Submit it again, ask for a human check, or continue: this proof comes first in your next session.',
  closed: 'This part of the lesson is closed.',
  assisted: 'Closed with help: you read the solution, so it earns less XP and comes back for review sooner.',
} as const;

/** What the screen shows. */
type Stage =
  | { kind: 'write'; mode: 'draft' | 'revise' | 'rewrite' | 'retry'; chain: ProofChain | null }
  | { kind: 'grading'; jobId: string }
  | { kind: 'loading'; jobId: string }
  | { kind: 'verdict'; chain: ProofChain; job: ProofGradingPoll }
  | { kind: 'reveal'; chain: ProofChain }
  | { kind: 'closed'; next: ServedProblem | null; nextUnavailable: boolean; xp?: number; assisted: boolean };

/** The stage a served proof starts at. A settled head is read through `seen` first. */
function initialStage(problem: ServedProblem): Stage {
  const proof = problem.proof!;
  if (proof.phase === 'grading' && proof.job_id) return { kind: 'grading', jobId: proof.job_id };
  if (proof.phase === 'rewrite') return { kind: 'write', mode: 'rewrite', chain: null };
  if (proof.job_id && proof.phase !== 'draft' && proof.phase !== 'closed') {
    return { kind: 'loading', jobId: proof.job_id };
  }
  return { kind: 'write', mode: 'draft', chain: null };
}

/** The stage the `seen` reply of a settled head moves to. */
function stageOf(chain: ProofChain, job: ProofGradingPoll): Stage {
  if (chain.solution && (chain.phase === 'rewrite' || chain.phase === 'reveal')) {
    return { kind: 'reveal', chain };
  }
  if (chain.phase === 'rewrite') return { kind: 'write', mode: 'rewrite', chain };
  return { kind: 'verdict', chain, job };
}

export interface LessonProofProps {
  api: ApiClient;
  call: Call;
  life: Lifetime;
  taskId: string;
  problem: ServedProblem;
  /** The point closed: hand the next problem (or the end of the task) to the session. */
  onClosed: (next: ServedProblem | null, nextUnavailable: boolean) => void;
  /** Leave the proof for later; the plan serves it first next time. */
  onLeave: () => void;
}

export function LessonProof({ api, call, life, taskId, problem, onClosed, onLeave }: LessonProofProps) {
  const [stage, setStage] = useState<Stage>(() => initialStage(problem));
  const [busy, setBusy] = useState(false);

  // A settled head on screen: read it through `seen`, which records the verdict as seen
  // and, at the cap, hands the solution over once.
  const readJob = stage.kind === 'loading' ? stage.jobId : null;
  useEffect(() => {
    if (readJob === null) return;
    void call(() => api.proofSeen(readJob), (seen) => {
      if (!life.alive()) return;
      setStage(stageOf(seen.chain, seen.job));
    });
  }, [api, call, life, readJob]);

  const submit = (text: string): void => {
    if (busy || !text.trim()) return;
    setBusy(true);
    void call(
      () => api.taskAnswer(taskId, { problem_id: problem.problem_id, answer: text }),
      (reply) => {
        if (!life.alive()) return;
        setBusy(false);
        const res = reply as AnswerResponse;
        if (res.task_status === 'proof_pending' && res.proof_grading) {
          setStage({ kind: 'grading', jobId: res.proof_grading.id });
          return;
        }
        setStage({
          kind: 'closed',
          next: res.next,
          nextUnavailable: !!res.next_unavailable,
          ...(res.xp === undefined ? {} : { xp: res.xp }),
          assisted: true,
        });
      },
      { onFail: () => { setBusy(false); } },
    );
  };

  const closePoint = (): void => {
    if (busy) return;
    setBusy(true);
    void call(() => api.taskProofContinue(taskId), (reply) => {
      if (!life.alive()) return;
      setBusy(false);
      setStage({
        kind: 'closed',
        next: reply.next,
        nextUnavailable: !!reply.next_unavailable,
        ...(reply.xp === undefined ? {} : { xp: reply.xp }),
        assisted: false,
      });
    }, { onFail: () => { setBusy(false); } });
  };

  return (
    <div className="card problem-card lesson-proof" data-stage={stage.kind}>
      <MathBlock>{problem.text}</MathBlock>
      <MathVisuals visuals={problem.visuals} />
      <Body
        api={api}
        call={call}
        life={life}
        stage={stage}
        busy={busy}
        setStage={setStage}
        onSubmit={submit}
        onClosePoint={closePoint}
        onClosed={onClosed}
        onLeave={onLeave}
      />
    </div>
  );
}

interface BodyProps {
  api: ApiClient;
  call: Call;
  life: Lifetime;
  stage: Stage;
  busy: boolean;
  setStage: (stage: Stage) => void;
  onSubmit: (text: string) => void;
  onClosePoint: () => void;
  onClosed: (next: ServedProblem | null, nextUnavailable: boolean) => void;
  onLeave: () => void;
}

function Body({ api, call, life, stage, busy, setStage, onSubmit, onClosePoint, onClosed, onLeave }: BodyProps) {
  switch (stage.kind) {
    case 'loading':
      return <p className="muted" role="status">Reading your proof's verdict…</p>;
    case 'grading':
      return <Grading api={api} life={life} jobId={stage.jobId} onLanded={() => { setStage({ kind: 'loading', jobId: stage.jobId }); }} onLeave={onLeave} />;
    case 'write':
      return <Write stage={stage} busy={busy} onSubmit={onSubmit} onLeave={onLeave} />;
    case 'reveal':
      return (
        <div className="proof-reveal">
          <p role="status">{LESSON_PROOF_TEXT.reveal}</p>
          <Solution text={stage.chain.solution!} />
          <div className="actions">
            <button type="button" className="btn btn-primary"
              onClick={() => { setStage({ kind: 'write', mode: 'rewrite', chain: null }); }}>
              I have read it — rewrite the proof without it
            </button>
          </div>
        </div>
      );
    case 'verdict':
      return <Verdict api={api} call={call} stage={stage} busy={busy} onSubmit={onSubmit} onClosePoint={onClosePoint} onLeave={onLeave} />;
    case 'closed':
      return (
        <div className="proof-closed">
          <p role="status">{stage.assisted ? LESSON_PROOF_TEXT.assisted : LESSON_PROOF_TEXT.closed}</p>
          {stage.xp != null ? <p className="chip chip-xp">{`+${String(stage.xp)} XP`}</p> : null}
          <div className="actions">
            <button type="button" className="btn btn-primary" onClick={() => { onClosed(stage.next, stage.nextUnavailable); }}>
              Continue →
            </button>
          </div>
        </div>
      );
  }
}

/** The obligations checklist. */
export function Obligations() {
  return (
    <ul className="proof-obligations">
      {PROOF_OBLIGATIONS.map((line) => <li key={line}>{line}</li>)}
    </ul>
  );
}

/** The textarea of a draft, a revision, a retry or the unaided rewrite. */
function Write({ stage, busy, onSubmit, onLeave }: {
  stage: Extract<Stage, { kind: 'write' }>;
  busy: boolean;
  onSubmit: (text: string) => void;
  onLeave: () => void;
}) {
  const prefill = stage.mode === 'revise' || stage.mode === 'retry' ? stage.chain?.draft ?? '' : '';
  const [text, setText] = useState(prefill);
  const label = {
    draft: 'Submit proof',
    revise: `Resubmit (revision ${String((stage.chain?.revision ?? 0) + 1)} of ${String(stage.chain?.cap ?? 2)})`,
    retry: 'Submit again',
    rewrite: 'Submit the rewrite',
  }[stage.mode];
  return (
    <div className="proof-write">
      {stage.mode === 'rewrite' ? <p role="status">{LESSON_PROOF_TEXT.rewrite}</p> : <Obligations />}
      <textarea
        className="work-input proof-input"
        rows={10}
        aria-label="Your proof"
        placeholder="Write your proof…"
        value={text}
        disabled={busy}
        onChange={(e) => { setText(e.target.value); }}
      />
      <div className="actions">
        <button type="button" className="btn btn-primary" disabled={busy || !text.trim()} onClick={() => { onSubmit(text); }}>
          {label}
        </button>
        {stage.mode === 'retry' ? (
          <button type="button" className="btn btn-ghost" onClick={onLeave}>Continue with the next task</button>
        ) : null}
      </div>
    </div>
  );
}

/** The wait: the obligations, the poll, and after 90 s the way out. */
function Grading({ api, life, jobId, onLanded, onLeave }: {
  api: ApiClient;
  life: Lifetime;
  jobId: string;
  onLanded: () => void;
  onLeave: () => void;
}) {
  const state = useProofGrading(api, life, { id: jobId, status: 'pending' });
  const [canLeave, setCanLeave] = useState(false);
  useEffect(() => {
    const timer = life.setTimeout(() => { setCanLeave(true); }, PROOF_LEAVE_AFTER_MS);
    return () => { life.clearTimer(timer); };
  }, [life, jobId]);
  const landed = state !== null && state.status !== 'pending' && state.status !== 'slow';
  useEffect(() => { if (landed) onLanded(); }, [landed, onLanded]);
  return (
    <div className="proof-grading" aria-live="polite">
      <p className="feedback-title">Checking your proof…</p>
      <p role="status">{LESSON_PROOF_TEXT.grading}</p>
      <Obligations />
      {canLeave ? (
        <>
          <p className="muted">{LESSON_PROOF_TEXT.leave}</p>
          <div className="actions">
            <button type="button" className="btn" onClick={onLeave}>Continue with the next task</button>
          </div>
        </>
      ) : null}
    </div>
  );
}

/** The verdict of a settled head: pass, needs revision, or not gradeable. */
function Verdict({ api, call, stage, busy, onSubmit, onClosePoint, onLeave }: {
  api: ApiClient;
  call: Call;
  stage: Extract<Stage, { kind: 'verdict' }>;
  busy: boolean;
  onSubmit: (text: string) => void;
  onClosePoint: () => void;
  onLeave: () => void;
}) {
  const { chain, job } = stage;
  if (chain.phase === 'passed') {
    return (
      <div className="proof-verdict" data-verdict="pass">
        <p className="feedback-title">Proof accepted</p>
        <p role="status">{LESSON_PROOF_TEXT.pass}</p>
        {job.feedback ? <MathBlock className="proof-feedback">{job.feedback}</MathBlock> : null}
        <Checks checks={job.checks ?? []} />
        {chain.solution ? <Solution text={chain.solution} /> : null}
        <div className="actions">
          <button type="button" className="btn btn-primary" disabled={busy} onClick={onClosePoint}>Continue →</button>
        </div>
      </div>
    );
  }
  if (chain.phase === 'unavailable') {
    // A grading that never landed (the daily limit, a failed check) never strands the
    // point: submit again for free, leave it for later (the plan carries it), or send it
    // to a human check.
    return (
      <div className="proof-verdict" data-verdict="unavailable">
        <p role="status">{LESSON_PROOF_TEXT.unavailable}</p>
        <Dispute api={api} call={call} jobId={chain.head_id} disputed={job.disputed === true} />
        <Write stage={{ kind: 'write', mode: 'retry', chain }} busy={busy} onSubmit={onSubmit} onLeave={onLeave} />
      </div>
    );
  }
  return (
    <div className="proof-verdict" data-verdict="needs_revision">
      <p className="feedback-title">Needs revision</p>
      <p role="status">{LESSON_PROOF_TEXT.revise}</p>
      {chain.feedback ? <MathBlock className="proof-feedback">{chain.feedback}</MathBlock> : null}
      <FirstUnmet unmet={chain.first_unmet ?? null} />
      <details className="proof-all-checks">
        <summary>All checks</summary>
        <Checks checks={job.checks ?? []} />
      </details>
      <Dispute api={api} call={call} jobId={chain.head_id} disputed={job.disputed === true} />
      <Write stage={{ kind: 'write', mode: 'revise', chain }} busy={busy} onSubmit={onSubmit} onLeave={onLeave} />
    </div>
  );
}

/** The check to fix first, with the learner's own words. */
export function FirstUnmet({ unmet }: { unmet: ProofUnmet | null }) {
  if (!unmet) return null;
  return (
    <div className="proof-first-unmet">
      <div className="solution-label">Fix this first</div>
      <p className="proof-check-text">{unmet.text}</p>
      <p className="proof-check-evidence muted">
        {unmet.evidence === 'not found' ? 'Not found in your proof.' : <>Your words: <q>{unmet.evidence}</q></>}
      </p>
    </div>
  );
}

/** Every check, met or not. */
export function Checks({ checks }: { checks: ProofCheck[] }) {
  if (!checks.length) return null;
  return (
    <ul className="proof-checks">
      {checks.map((check) => (
        <li key={check.id} className={check.met ? 'proof-check-met' : 'proof-check-unmet'}>
          <span className="proof-check-answer">{check.met ? 'Yes' : 'No'}</span>{' '}
          <span className="proof-check-text">{check.text}</span>
          <div className="proof-check-evidence muted">
            {check.evidence === 'not found' ? 'Not found in your proof.' : <q>{check.evidence}</q>}
          </div>
        </li>
      ))}
    </ul>
  );
}

/** The reference solution. */
export function Solution({ text }: { text: string }) {
  return (
    <div className="solution">
      <div className="solution-label">Solution</div>
      <MathBlock className="solution-text">{text}</MathBlock>
    </div>
  );
}

/** "This grade is wrong": the grading goes to a human. */
export function Dispute({ api, call, jobId, disputed }: { api: ApiClient; call: Call; jobId: string; disputed: boolean }) {
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState('');
  const [sent, setSent] = useState(disputed);
  if (sent) return <p className="muted proof-disputed" role="status">Sent for a human check. The grade changes if the check disagrees.</p>;
  if (!open) {
    return <button type="button" className="btn btn-ghost proof-dispute" onClick={() => { setOpen(true); }}>This grade is wrong</button>;
  }
  return (
    <div className="proof-dispute-form">
      <textarea className="work-input" rows={3} aria-label="Why the grade is wrong (optional)"
        placeholder="Why the grade is wrong (optional)…" value={note}
        onChange={(e) => { setNote(e.target.value); }} />
      <div className="actions">
        <button type="button" className="btn" onClick={() => {
          const trimmed = note.trim();
          void call(() => api.proofDispute(jobId, trimmed ? trimmed : undefined), () => { setSent(true); });
        }}>
          Send for a human check
        </button>
      </div>
    </div>
  );
}
