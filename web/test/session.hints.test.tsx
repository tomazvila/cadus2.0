/**
 * The hint panel and the hint button of the study loop (H-3).
 *
 * A serve with `hint_available: false` names a knowledge point with no approved hint ladder.
 * The hint route can only refuse that request, so the view shows no hint control.
 */
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { HintButton, HintPanel } from '@/views/session/Hints';
import type { ApiClient } from '@/api/types';
import { P, answerInput, mount, stubApi } from './helpers/session';

const NO_LADDER = /No hints are written for this one yet/;

describe('HintPanel', () => {
  it('numbers the hints in the order given', () => {
    render(<HintPanel hints={['first', 'second']} referenceLesson={null} hintsAvailable taskType="review" />);
    const hints = [...document.querySelectorAll('.hint')].map((h) => h.textContent);
    expect(hints).toEqual(['Hint 1: first', 'Hint 2: second']);
    expect(document.querySelector('.reference-lesson')).toBeNull();
  });

  it('names the reference lesson one time', () => {
    render(<HintPanel hints={[]} referenceLesson="Fractions" hintsAvailable taskType="review" />);
    const notes = [...document.querySelectorAll('.reference-lesson')].map((n) => n.textContent);
    expect(notes).toEqual([
      'Still stuck? This is a review — re-study the lesson “Fractions”, then answer as best you can.',
    ]);
  });

  it('tells a review with no hint ladder to study the lesson again', () => {
    render(<HintPanel hints={[]} referenceLesson={null} hintsAvailable={false} taskType="review" />);
    expect(screen.getByText(NO_LADDER)).toBeTruthy();
  });

  it('shows no such note for a lesson, and none for a review that has a ladder', () => {
    const view = render(<HintPanel hints={[]} referenceLesson={null} hintsAvailable={false} taskType="lesson" />);
    expect(screen.queryByText(NO_LADDER)).toBeNull();
    view.rerender(<HintPanel hints={[]} referenceLesson={null} hintsAvailable taskType="review" />);
    expect(screen.queryByText(NO_LADDER)).toBeNull();
  });
});

describe('HintButton', () => {
  it('shows nothing when hidden', () => {
    const view = render(<HintButton hidden locked={false} onClick={vi.fn()} />);
    expect(view.container.innerHTML).toBe('');
  });

  it('calls onClick, and follows the locked state', () => {
    const onClick = vi.fn();
    const view = render(<HintButton hidden={false} locked={false} onClick={onClick} />);
    fireEvent.click(screen.getByRole('button', { name: 'Hint' }));
    expect(onClick).toHaveBeenCalledTimes(1);
    view.rerender(<HintButton hidden={false} locked onClick={onClick} />);
    expect((screen.getByRole('button', { name: 'Hint' }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe('the session with `hint_available: false`', () => {
  it('shows no Hint button, shows the note of a review, and the H key asks for no hint', async () => {
    const taskHint = vi.fn<ApiClient['taskHint']>(async () => ({ hint: 'unused', hint_number: 1 }));
    await mount({ api: stubApi({ taskServe: async () => P(1, { hint_available: false }), taskHint }) });
    expect(screen.queryByRole('button', { name: 'Hint' })).toBeNull();
    expect(screen.getByText(NO_LADDER)).toBeTruthy();
    fireEvent.keyDown(answerInput(), { key: 'h' });
    expect(taskHint).not.toHaveBeenCalled();
  });
});
