/**
 * D-PR1: a written proof inside a lesson. The knowledge point closes on a PASS, not on the
 * submission: the screen waits for the verdict, takes revisions with the last draft
 * prefilled, hides the solution until a pass or the cap, and after the cap asks for one
 * unaided rewrite.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import { LESSON, REVIEW, TEACHING, P, graded, mount, planOf, press, stubApi } from './helpers/session';
import { LESSON_PROOF_TEXT, LESSON_WRITTEN_TEXT, PROOF_LEAVE_AFTER_MS, PROOF_OBLIGATIONS } from '@/views/session/LessonProof';
import type {
  AnswerResponse, ApiClient, ProofChain, ProofField, ProofGradingPoll, ProofSeenResponse, ServedProblem,
} from '@/api/types';

const SOLUTION = 'Let n = 2k. Then n^2 = 2(2k^2).';
const DRAFT = 'n is even so n squared is even.';

const proof = (over: Partial<ProofField> = {}): ProofField => ({
  context: 'lesson', phase: 'draft', revision: 0, cap: 2, revisions_left: 2, ...over,
});

const proofProblem = (over: Partial<ProofField> = {}): ServedProblem =>
  P(1, { text: 'Prove that the square of an even integer is even.', total: null, proof: proof(over) });

const chain = (over: Partial<ProofChain> = {}): ProofChain => ({
  root_id: 'job-1', head_id: 'job-1', context: 'lesson', task_id: 't-lesson', topic: 'fractions',
  kp: 'kp-simplify', problem: 'Prove it.', phase: 'revise', revision: 0, cap: 2, revisions_left: 2,
  passed: false, draft: DRAFT, versions: [], feedback: 'Justify the last step.',
  first_unmet: { id: 'G3', text: 'Each step is justified.', evidence: 'so n squared is even' },
  ...over,
});

const job = (over: Partial<ProofGradingPoll> = {}): ProofGradingPoll => ({
  id: 'job-1', attempt_id: 'a-1', status: 'needs_revision', feedback: 'Justify the last step.',
  checks: [{ id: 'G3', text: 'Each step is justified.', met: false, minor: false, evidence: 'so n squared is even' }],
  ...over,
});

const pending = (id: string): AnswerResponse => graded({
  outcome: 'ungraded', reason: 'no deterministic verdict for a proof', task_status: 'proof_pending',
  next: null, proof_grading: { id, status: 'pending' }, proof: proof({ phase: 'grading', job_id: id }),
});

const textarea = () => screen.getByLabelText('Your proof') as HTMLTextAreaElement;

async function write(text: string, button: string | RegExp): Promise<void> {
  await act(async () => { fireEvent.change(textarea(), { target: { value: text } }); });
  await press(button);
}

/** Mount a lesson whose serve is the proof, and get past the teach page. */
async function mountProof(api: Partial<ApiClient>, served: ServedProblem = proofProblem()) {
  const view = await mount({
    plan: planOf(LESSON, REVIEW),
    api: stubApi({ taskTeach: async () => TEACHING, taskServe: async () => served, ...api }),
  });
  await press("I've got it — practice ▸");
  return view;
}

afterEach(() => { vi.useRealTimers(); });

describe('a lesson proof', () => {
  it('waits for the verdict, takes a revision with the last draft, and closes on the pass', async () => {
    let landed: ProofGradingPoll = job({ id: 'job-1' });
    const getProofGrading = vi.fn<ApiClient['getProofGrading']>(async () => landed);
    const proofSeen = vi.fn<ApiClient['proofSeen']>(async (id): Promise<ProofSeenResponse> => (
      id === 'job-1'
        ? { job: job(), chain: chain() }
        : { job: job({ id: 'job-2', status: 'pass', feedback: 'Complete.', checks: [] }),
            chain: chain({ head_id: 'job-2', phase: 'passed', passed: true, solution: SOLUTION }) }
    ));
    const answers = [pending('job-1'), pending('job-2')];
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => answers.shift()!);
    const taskProofContinue = vi.fn<ApiClient['taskProofContinue']>(async () => ({
      task_status: 'task_passed', remediation: [], next: null, xp: 12,
    }));
    await mountProof({ getProofGrading, proofSeen, taskAnswer, taskProofContinue });

    // The draft: the obligations, an empty textarea, no solution.
    for (const line of PROOF_OBLIGATIONS) expect(screen.getByText(line)).toBeTruthy();
    expect(textarea().value).toBe('');
    await write(DRAFT, 'Submit proof');
    expect(taskAnswer.mock.calls[0]).toEqual(['t-lesson', { problem_id: 'p1', answer: DRAFT }]);

    // The verdict lands (the wait and its checklist are the next test's) and is read
    // through `seen`.
    await waitFor(() => expect(screen.getByText('Needs revision')).toBeTruthy());
    expect(proofSeen).toHaveBeenCalledWith('job-1');
    expect(screen.getByText('Fix this first')).toBeTruthy();
    expect(document.querySelector('.proof-first-unmet q')!.textContent).toBe('so n squared is even');
    expect(textarea().value).toBe(DRAFT);
    expect(screen.queryByText(SOLUTION)).toBeNull();
    expect(screen.queryByText('Continue →')).toBeNull();

    // The revision goes to the same problem.
    landed = job({ id: 'job-2', status: 'pass' });
    await write(`${DRAFT} Since n = 2k, n^2 = 2(2k^2).`, /Resubmit/);
    expect(taskAnswer.mock.calls[1]![1].problem_id).toBe('p1');
    await waitFor(() => expect(screen.getByText('Proof accepted')).toBeTruthy());
    expect(screen.getByText(SOLUTION)).toBeTruthy();

    // Continue closes the point with its XP, then the loop moves to the next task.
    await press('Continue →');
    expect(taskProofContinue).toHaveBeenCalledWith('t-lesson');
    expect(screen.getByText(LESSON_PROOF_TEXT.closed)).toBeTruthy();
    expect(screen.getByText('+12 XP')).toBeTruthy();
  });

  it('offers to leave the proof for later after 90 seconds of grading', async () => {
    vi.useFakeTimers();
    const getProofGrading = vi.fn<ApiClient['getProofGrading']>(async () => job({ status: 'pending' }));
    const taskServe = vi.fn<ApiClient['taskServe']>(async (taskId) => (taskId === 't-lesson' ? proofProblem({ phase: 'grading', job_id: 'job-1' }) : P(1)));
    await mount({
      plan: planOf(LESSON, REVIEW),
      api: stubApi({ taskTeach: async () => TEACHING, taskServe, getProofGrading }),
    });
    await press("I've got it — practice ▸");
    expect(screen.getByText(LESSON_PROOF_TEXT.grading)).toBeTruthy();
    expect(screen.queryByText('Continue with the next task')).toBeNull();
    await act(async () => { vi.advanceTimersByTime(PROOF_LEAVE_AFTER_MS); });
    expect(screen.getByText(LESSON_PROOF_TEXT.leave)).toBeTruthy();
    await press('Continue with the next task');
    expect(taskServe.mock.calls.at(-1)).toEqual(['t-review']);
  });

  it('at the cap shows the solution once, then takes an unaided rewrite that closes the point', async () => {
    const proofSeen = vi.fn<ApiClient['proofSeen']>(async () => ({
      job: job({ id: 'job-3', revision: 2 }),
      chain: chain({ head_id: 'job-3', phase: 'rewrite', revision: 2, revisions_left: 0, solution: SOLUTION }),
    }));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded({
      outcome: 'ungraded', task_status: 'task_passed', next: null, xp: 6,
      proof_grading: { id: 'job-4', status: 'pending' }, proof: proof({ phase: 'closed', rewrite: true }),
    }));
    await mountProof({ proofSeen, taskAnswer }, proofProblem({ phase: 'reveal', job_id: 'job-3', revision: 2, revisions_left: 0 }));

    await waitFor(() => expect(screen.getByText(SOLUTION)).toBeTruthy());
    expect(screen.getByText(LESSON_PROOF_TEXT.reveal)).toBeTruthy();
    await press(/rewrite the proof without it/);
    expect(screen.queryByText(SOLUTION)).toBeNull();
    expect(screen.getByText(LESSON_PROOF_TEXT.rewrite)).toBeTruthy();
    expect(textarea().value).toBe('');
    await write('Let n = 2k; n^2 = 2(2k^2), which is even.', 'Submit the rewrite');
    expect(screen.getByText(LESSON_PROOF_TEXT.assisted)).toBeTruthy();
    expect(screen.getByText('+6 XP')).toBeTruthy();
  });

  it('sends a disputed grade for a human check', async () => {
    const proofDispute = vi.fn<ApiClient['proofDispute']>(async () => job({ disputed: true }));
    const proofSeen = vi.fn<ApiClient['proofSeen']>(async () => ({ job: job(), chain: chain() }));
    await mountProof({ proofSeen, proofDispute }, proofProblem({ phase: 'revise', job_id: 'job-1' }));
    await waitFor(() => expect(screen.getByText('Needs revision')).toBeTruthy());
    await press('This grade is wrong');
    await act(async () => {
      fireEvent.change(screen.getByLabelText('Why the grade is wrong (optional)'), { target: { value: 'Step 2 is fine.' } });
    });
    await press('Send for a human check');
    expect(proofDispute).toHaveBeenCalledWith('job-1', 'Step 2 is fine.');
    expect(screen.getByText(/Sent for a human check/)).toBeTruthy();
  });
});

describe('a lesson proof the grader could not grade', () => {
  it('offers a dispute, a free resubmission and a way on, so the point is never stranded', async () => {
    const proofSeen = vi.fn<ApiClient['proofSeen']>(async () => ({
      job: { id: 'job-1', attempt_id: 'a-1', status: 'capped' },
      chain: chain({ phase: 'unavailable', feedback: '', first_unmet: null }),
    }));
    const proofDispute = vi.fn<ApiClient['proofDispute']>(async () => job({ status: 'capped', disputed: true }));
    const taskServe = vi.fn<ApiClient['taskServe']>(async (taskId) => (
      taskId === 't-lesson' ? proofProblem({ phase: 'unavailable', job_id: 'job-1' }) : P(1)));
    await mount({
      plan: planOf(LESSON, REVIEW),
      api: stubApi({ taskTeach: async () => TEACHING, taskServe, proofSeen, proofDispute }),
    });
    await press("I've got it — practice ▸");
    await waitFor(() => expect(screen.getByText(LESSON_PROOF_TEXT.unavailable)).toBeTruthy());
    expect(textarea().value).toBe(DRAFT);
    expect(screen.getByRole('button', { name: 'Submit again' })).toBeTruthy();
    await press('This grade is wrong');
    await press('Send for a human check');
    expect(proofDispute).toHaveBeenCalledWith('job-1', undefined);
    await press('Continue with the next task');
    expect(taskServe.mock.calls.at(-1)).toEqual(['t-review']);
  });
});

describe('a written answer', () => {
  it('words the loop for an answer, not a proof', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => pending('job-1'));
    const served = P(1, {
      text: 'Write the contrapositive of: if n is even, n squared is even.',
      total: null, written: true, proof: proof(),
    });
    await mountProof({ taskAnswer }, served);

    // One sentence is the answer: no proof obligations, no proof wording.
    for (const line of PROOF_OBLIGATIONS) expect(screen.queryByText(line)).toBeNull();
    expect(screen.getByText(LESSON_WRITTEN_TEXT.hint)).toBeTruthy();
    expect(screen.queryByLabelText('Your proof')).toBeNull();
    expect(screen.queryByText('Submit proof')).toBeNull();
    const area = screen.getByLabelText('Your answer') as HTMLTextAreaElement;
    await act(async () => { fireEvent.change(area, { target: { value: 'If n squared is odd, n is odd.' } }); });
    await press('Submit answer');
    expect(taskAnswer.mock.calls[0]![1].answer).toBe('If n squared is odd, n is odd.');
    expect(screen.getByText('Checking your answer…')).toBeTruthy();
  });
});
