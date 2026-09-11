/**
 * The two verdict panels and the diagnosis, rendered on their own.
 *
 * The session tests reach these through the whole view; this part pins the class names,
 * the chips and the rows each panel paints from one reply.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { createLifetime } from '@/hooks/useLifetime';
import { Feedback } from '@/views/session/Feedback';
import { DIAGNOSIS_FAILED, DIAGNOSIS_WAIT, Diagnosis } from '@/views/session/Diagnosis';
import { DiagnosisStore } from '@/views/session/useDiagnosis';
import { graded, ungraded } from './helpers/session';
import type { ApiClient, DiagnosisField } from '@/api/types';

const feedback = () => document.querySelector('.feedback')!;

describe('Feedback', () => {
  it('separates a correct practice answer from an unsuccessful assessment', () => {
    render(<Feedback res={graded({ task_status: 'task_failed' })} hasNext={false} onContinue={vi.fn()} onEnd={vi.fn()} />);
    expect(screen.getByText(/The original assessment still needs more practice/)).toBeTruthy();
  });
  it('names a fresh-item block while preserving a retry action', () => {
    render(<Feedback res={graded({ feedback_practice: true, feedback_blocked: true })} hasNext onContinue={vi.fn()} onEnd={vi.fn()} />);
    expect(screen.getByRole('status').textContent).toContain('Your answer is saved');
    expect(screen.getByRole('button', { name: 'Check for fresh practice →' })).toBeTruthy();
  });
  it('shows an inconclusive review separately from the last answer', () => {
    render(<Feedback res={graded({ task_status: 'task_inconclusive' })} hasNext={false} onContinue={vi.fn()} onEnd={vi.fn()} />);
    expect(screen.getByText(/This review needs confirmation/)).toBeTruthy();
  });
  it('classes the panel by the verdict, and chips the quality and the XP', () => {
    render(<Feedback res={graded({ work_quality: 'nearly_perfect' })} hasNext onContinue={vi.fn()} onEnd={vi.fn()} />);
    expect(feedback().className).toBe('feedback feedback-correct');
    expect(document.querySelector('.chip-quality')!.textContent).toBe('nearly perfect');
    expect(document.querySelector('.chip-xp')!.textContent).toBe('+10 XP');
  });

  it('classes a miss as incorrect, and chips no XP when the reply carries none', () => {
    const res = graded({ correct: false });
    delete res.xp;
    render(<Feedback res={res} hasNext onContinue={vi.fn()} onEnd={vi.fn()} />);
    expect(feedback().className).toBe('feedback feedback-incorrect');
    expect(document.querySelector('.chip-xp')).toBeNull();
  });

  it('paints an ungraded attempt as "Not marked", with no red and no verdict wording', () => {
    render(
      <Feedback
        res={ungraded()}
        hasNext
        onContinue={vi.fn()}
        onEnd={vi.fn()}
      >
        <div className="diagnosis">the model prose</div>
      </Feedback>,
    );
    // D-F2: the panel is neutral. It is not the miss panel and it carries no tier.
    expect(feedback().className).toBe('feedback feedback-ungraded');
    expect(document.querySelector('.chip-quality')).toBeNull();
    expect(document.querySelector('.chip-xp')).toBeNull();
    expect(screen.getByText('Not marked')).toBeTruthy();
    expect(screen.queryByText('Not quite')).toBeNull();
    // The service's own reason, and nothing this component wrote.
    expect(document.querySelector('.feedback-reason')!.textContent)
      .toBe('the answer left the grammar');
    // Hard Rule 1: no answer reveal. D-F4: no model prose on an ungraded attempt.
    expect(document.querySelector('.solution')).toBeNull();
    expect(document.querySelector('.re-solve')).toBeNull();
    expect(document.querySelector('.diagnosis')).toBeNull();
    // The learner still has a way on.
    expect(screen.getByRole('button', { name: 'Next problem →' })).toBeTruthy();
  });

  it('lists every follow-up as its kind and its targets', () => {
    render(
      <Feedback
        res={graded({
          remediation: [
            { kind: 'review', targets: ['fractions', 'decimals'] },
            { kind: 'lesson', targets: ['ratios'] },
          ],
        })}
        hasNext
        onContinue={vi.fn()}
        onEnd={vi.fn()}
      />,
    );
    expect(Array.from(document.querySelectorAll('.remediation li')).map((li) => li.textContent))
      .toEqual(['review: fractions, decimals', 'lesson: ratios']);
  });
});

describe('Diagnosis, on its own', () => {
  function storeWith(over: Partial<ApiClient> = {}) {
    vi.useFakeTimers();
    const api: ApiClient = { ...createDemoApi(), ...over };
    return { api, store: new DiagnosisStore({ api, life: createLifetime() }) };
  }

  it('renders nothing for a null field, and throws nothing', () => {
    const { store } = storeWith();
    const field: DiagnosisField = null;
    render(<Diagnosis store={store} field={field} />);
    expect(document.querySelector('.diagnosis')).toBeNull();
  });

  it('polls for nothing when the diagnosis came ready with the grade', async () => {
    const getDiagnosis = vi.fn<ApiClient['getDiagnosis']>();
    const { store } = storeWith({ getDiagnosis });
    render(<Diagnosis store={store} field={{ status: 'ready', error_tags: ['sign'], prose: 'Because.' }} />);
    expect(screen.getByText('Because.')).toBeTruthy();
    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(getDiagnosis).not.toHaveBeenCalled();
  });

  it('says it is working, and not that it failed, while the job is pending', () => {
    const { store } = storeWith();
    render(<Diagnosis store={store} field={{ status: 'pending', id: 'j1' }} />);
    expect(screen.getByText(DIAGNOSIS_WAIT)).toBeTruthy();
    expect(screen.queryByText(DIAGNOSIS_FAILED)).toBeNull();
  });
});
