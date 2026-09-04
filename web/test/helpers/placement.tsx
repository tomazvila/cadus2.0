/**
 * The fixtures and the moves of the placement tests.
 *
 * The literals come from the 1.0 view and spec section 6: the beat is 750 ms, the cap
 * default is 40, and the progress line reads `Question 1 of up to 40`. The payloads are the
 * frozen shapes of the three `/api/diag/*` rows.
 */
import { vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { Diagnostic, type DiagnosticProps } from '@/views/Diagnostic';
import { resetToasts, toastStore } from '@/app/toast';
import { renderInView } from './render';
import type { DiagFinishResponse, DiagProbe, DiagStartResponse, DiagnosticApi } from '@/api/diag';

export const probe = (over: Partial<DiagProbe> = {}): DiagProbe => ({
  problem_id: 'd1',
  topic: 'Adding integers',
  text: 'Work out $-7 + 12$.',
  ...over,
});

export const START: DiagStartResponse = { probe: probe(), asked: 0, cap: 40 };

export const SUMMARY: DiagFinishResponse = { placed: ['a', 'b'], conditional: [], frontier: ['c'] };

/** A port that starts on `START`, grades every answer correct and ends on `SUMMARY`. */
export function stubDiag(over: Partial<DiagnosticApi> = {}): DiagnosticApi {
  return {
    diagStart: async () => START,
    diagAnswer: async () => ({ correct: true, next_probe: { done: true } }),
    diagFinish: async () => SUMMARY,
    ...over,
  };
}

/** The element the placement renders, with its two navigation spies. */
export function placementProps(over: Partial<DiagnosticProps> = {}) {
  const handlers = { onUnauthorized: vi.fn(), onExit: vi.fn() };
  const props: DiagnosticProps = { diag: stubDiag(), ...handlers, ...over };
  return { props, handlers };
}

export async function mount(over: Partial<DiagnosticProps> = {}) {
  resetToasts();
  const { props, handlers } = placementProps(over);
  const view = await renderInView(<Diagnostic {...props} />);
  return { ...view, ...handlers };
}

export const answerInput = () => screen.getByLabelText('Answer') as HTMLInputElement;
export const beginButton = () => screen.getByRole('button', { name: 'Begin placement' });
export const submitButton = () => screen.getByRole('button', { name: 'Submit' });
export const skipButton = () => screen.getByRole('button', { name: 'Skip — I don’t know' });
export const probeText = () => document.querySelector('.problem-text')!.textContent;
export const progressCount = () => document.querySelector('.progress-count')!.textContent;
export const toasts = () => toastStore.getSnapshot();

/** Press Begin and let the start settle. */
export async function begin(): Promise<void> {
  await act(async () => { fireEvent.click(beginButton()); });
}

/** Answer the probe on screen. */
export async function answer(text: string): Promise<void> {
  fireEvent.change(answerInput(), { target: { value: text } });
  await act(async () => { fireEvent.click(submitButton()); });
}

/** Mount, begin, and answer the first probe. */
export async function answerFirst(over: Partial<DiagnosticProps> = {}) {
  const view = await mount(over);
  await begin();
  await answer('5');
  return view;
}
