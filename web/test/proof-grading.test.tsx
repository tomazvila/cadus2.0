/**
 * Amendment K point 6: the feedback panel of a written proof under background grading.
 *
 * The panel states, one per grading state, and the poll that moves the panel from
 * "Checking your proof…" to the verdict.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { createLifetime } from '@/hooks/useLifetime';
import { Feedback, ProofAwareFeedback } from '@/views/session/Feedback';
import { PROOF_POLL_MS, PROOF_TEXT, type ProofState } from '@/views/session/ProofGrading';
import { ungraded } from './helpers/session';
import type { ApiClient, ProofGradingPoll } from '@/api/types';

const feedback = () => document.querySelector('.feedback')!;
const title = () => document.querySelector('.feedback-title')!.textContent;

const CHECKS = [
  { id: 'G1', text: 'The claim is stated.', met: true, minor: false, evidence: 'Let a and b be odd' },
  { id: 'S2', text: 'Both odd integers get their own parameter.', met: false, minor: false, evidence: 'not found' },
];

const proofReply = () => ungraded({
  reason: 'no deterministic verdict for a proof',
  proof_grading: { id: 'job-1', status: 'pending' },
});

function panel(proof: ProofState | null) {
  render(<Feedback res={proofReply()} hasNext onContinue={vi.fn()} onEnd={vi.fn()} proof={proof} />);
}

afterEach(() => { vi.useRealTimers(); });

describe('the written-proof panel', () => {
  it('reads "Checking your proof…" while the grading is pending, and keeps Continue live', () => {
    panel({ status: 'pending' });
    expect(title()).toBe('Checking your proof…');
    expect(feedback().className).toBe('feedback feedback-ungraded');
    expect(screen.getByRole('status').textContent).toBe(PROOF_TEXT.pending);
    // The service's own reason gives way to the grading state.
    expect(screen.queryByText('no deterministic verdict for a proof')).toBeNull();
    expect(document.querySelector('.proof-checks')).toBeNull();
    expect(screen.getByRole('button', { name: 'Next problem →' })).toBeTruthy();
  });

  it('shows a pass with its checks, the quotes and the solution', () => {
    panel({ status: 'pass', feedback: 'A complete proof.', checks: [{ ...CHECKS[0]! }], solution: 'The worked proof.' });
    expect(title()).toBe('Proof accepted');
    expect(feedback().className).toBe('feedback feedback-correct');
    expect(screen.getByText(PROOF_TEXT.pass)).toBeTruthy();
    expect(screen.getByText('A complete proof.')).toBeTruthy();
    const items = Array.from(document.querySelectorAll('.proof-checks li'));
    expect(items).toHaveLength(1);
    expect(items[0]!.textContent).toContain('Yes');
    expect(items[0]!.querySelector('q')!.textContent).toBe('Let a and b be odd');
    expect(document.querySelector('.solution-text')!.textContent).toBe('The worked proof.');
  });

  it('shows needs revision with the feedback and every check, met or not', () => {
    panel({ status: 'needs_revision', feedback: 'Step 1 uses the same k for both integers.', checks: CHECKS });
    expect(title()).toBe('Needs revision');
    expect(feedback().className).toBe('feedback feedback-ungraded');
    expect(screen.getByText('Step 1 uses the same k for both integers.')).toBeTruthy();
    const unmet = document.querySelector('.proof-check-unmet')!;
    expect(unmet.textContent).toContain('No');
    expect(unmet.textContent).toContain('Both odd integers get their own parameter.');
    expect(unmet.textContent).toContain('Not found in your proof.');
  });

  it('says a capped or failed grading stays for a human review', () => {
    panel({ status: 'capped' });
    expect(title()).toBe('Not marked');
    expect(screen.getByRole('status').textContent).toBe(PROOF_TEXT.capped);
  });

  it('stays the plain "Not marked" panel when the reply carries no grading', () => {
    render(<Feedback res={ungraded()} hasNext onContinue={vi.fn()} onEnd={vi.fn()} proof={null} />);
    expect(title()).toBe('Not marked');
    expect(document.querySelector('.feedback-reason')!.textContent).toBe('the answer left the grammar');
  });
});

describe('the proof-grading poll', () => {
  it('polls until the grading lands, then paints the verdict and stops', async () => {
    vi.useFakeTimers();
    const pending: ProofGradingPoll = { id: 'job-1', attempt_id: 'a-1', status: 'pending' };
    const passed: ProofGradingPoll = {
      id: 'job-1', attempt_id: 'a-1', status: 'pass', feedback: 'Complete.', checks: [{ ...CHECKS[0]! }],
    };
    const getProofGrading = vi.fn<ApiClient['getProofGrading']>()
      .mockResolvedValueOnce(pending)
      .mockResolvedValue(passed);
    const api: ApiClient = { ...createDemoApi(), getProofGrading };
    const life = createLifetime();
    render(<ProofAwareFeedback api={api} life={life} res={proofReply()} hasNext onContinue={vi.fn()} onEnd={vi.fn()} />);

    expect(title()).toBe('Checking your proof…');
    await act(async () => { vi.advanceTimersByTime(PROOF_POLL_MS); });
    expect(title()).toBe('Checking your proof…');
    await act(async () => { vi.advanceTimersByTime(PROOF_POLL_MS); });
    expect(title()).toBe('Proof accepted');
    expect(getProofGrading).toHaveBeenCalledWith('job-1');
    const calls = getProofGrading.mock.calls.length;
    await act(async () => { vi.advanceTimersByTime(PROOF_POLL_MS * 3); });
    expect(getProofGrading.mock.calls.length).toBe(calls);
  });

  it('never polls for a reply that carries no grading', async () => {
    vi.useFakeTimers();
    const getProofGrading = vi.fn<ApiClient['getProofGrading']>();
    const api: ApiClient = { ...createDemoApi(), getProofGrading };
    render(<ProofAwareFeedback api={api} life={createLifetime()} res={ungraded()} hasNext onContinue={vi.fn()} onEnd={vi.fn()} />);
    await act(async () => { vi.advanceTimersByTime(PROOF_POLL_MS * 3); });
    expect(getProofGrading).not.toHaveBeenCalled();
    expect(title()).toBe('Not marked');
  });
});
