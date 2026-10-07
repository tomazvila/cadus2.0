/** The feedback panel while the background equivalence check runs, and when it lands. */
import { describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { createLifetime } from '@/hooks/useLifetime';
import { EQUIVALENCE_POLL_MS, Feedback, ProofAwareFeedback } from '@/views/session/Feedback';
import { graded } from './helpers/session';
import type { ApiClient, EquivalencePoll } from '@/api/types';

const pendingRes = () => graded({
  correct: false, work_quality: 'nearly_passable', xp: 1.05,
  error_tags: ['sign_error'], re_solve: 'Try again.',
  remediation: [{ kind: 'lesson_fail', targets: [] }],
  equivalence: { id: 'e1', status: 'pending' },
});
const text = () => document.querySelector('.feedback')!.textContent!;

describe('pending equivalence check', () => {
  it('paints a neutral panel with none of the failure parts', () => {
    render(<Feedback res={pendingRes()} hasNext onContinue={vi.fn()} onEnd={vi.fn()}>
      <div className="diagnosis">Working out what went wrong…</div>
    </Feedback>);
    expect(document.querySelector('.feedback')!.className).toBe('feedback feedback-pending');
    expect(text()).toContain('Checking your answer…');
    for (const gone of ['Not quite', 'Correct', 'nearly passable', 'XP', 'Follow-up', 'lesson_fail', 'Working out', 'Stop for now', 'fresh problem', 'Next problem']) {
      expect(text()).not.toContain(gone);
    }
    expect(document.querySelector('.feedback-mark')).toBeNull();
    const btn = screen.getByRole('button', { name: 'Waiting for the check…' });
    expect(btn.getAttribute('aria-disabled')).toBe('true');
    expect(screen.getAllByRole('button')).toHaveLength(1);
  });

  it('is replaced by the correct panel, with no stale chips, when the check lands', async () => {
    vi.useFakeTimers();
    try {
      const life = createLifetime();
      const api: ApiClient = { ...createDemoApi(), getEquivalence: async (): Promise<EquivalencePoll> =>
        ({ id: 'e1', attempt_id: 'a-1', verdict: { status: 'accepted' } }) };
      render(<ProofAwareFeedback api={api} life={life} res={pendingRes()} hasNext onContinue={vi.fn()} onEnd={vi.fn()}>
        <div className="diagnosis">Working out what went wrong…</div>
      </ProofAwareFeedback>);
      expect(text()).toContain('Checking your answer…');
      await act(async () => { await vi.advanceTimersByTimeAsync(EQUIVALENCE_POLL_MS + 10); });
      expect(document.querySelector('.feedback')!.className).toBe('feedback feedback-correct');
      expect(text()).toContain('Correct');
      expect(text()).toContain('Correct. Your progress is updated.');
      expect(document.querySelector('.chip-xp')).toBeNull();
      expect(document.querySelector('.chip-quality')).toBeNull();
      for (const gone of ['Checking your answer', 'Waiting for the check', 'Not quite', 'XP', 'nearly passable', 'lesson_fail', 'Working out', 'Follow-up', 'Try again.']) {
        expect(text()).not.toContain(gone);
      }
    } finally { vi.useRealTimers(); }
  });

  it('is replaced by the usual wrong panel when the check refuses', async () => {
    vi.useFakeTimers();
    try {
      const life = createLifetime();
      const api: ApiClient = { ...createDemoApi(), getEquivalence: async (): Promise<EquivalencePoll> =>
        ({ id: 'e1', attempt_id: 'a-1', verdict: { status: 'refused', reason: 'Not the same value.' } }) };
      render(<ProofAwareFeedback api={api} life={life} res={pendingRes()} hasNext onContinue={vi.fn()} onEnd={vi.fn()} />);
      await act(async () => { await vi.advanceTimersByTimeAsync(EQUIVALENCE_POLL_MS + 10); });
      expect(document.querySelector('.feedback')!.className).toBe('feedback feedback-incorrect');
      expect(text()).toContain('Not quite');
      expect(text()).toContain('Checked: Not the same value.');
      expect(text()).not.toContain('Checking your answer');
    } finally { vi.useRealTimers(); }
  });
});

describe('follow-up rows', () => {
  it('renders no raw key for a lesson_fail row with no targets', () => {
    render(<Feedback res={graded({ correct: false, remediation: [{ kind: 'lesson_fail', targets: [] }] })} hasNext onContinue={vi.fn()} onEnd={vi.fn()} />);
    expect(text()).toContain('This lesson comes back next time');
    expect(text()).not.toContain('lesson_fail');
  });
  it('hides a row with no text and no targets, and names the targets of a known kind', () => {
    render(<Feedback res={graded({ correct: false, remediation: [
      { kind: 'mystery', targets: [] }, { kind: 'repeat_fail', targets: ['subtraction'] },
    ] })} hasNext onContinue={vi.fn()} onEnd={vi.fn()} />);
    expect(document.querySelectorAll('.remediation li')).toHaveLength(1);
    expect(text()).toContain('Practice this skill again: subtraction');
    expect(text()).not.toContain('mystery');
  });
});
