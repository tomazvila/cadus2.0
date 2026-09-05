/**
 * The empty submit, the same on every study card: the field takes the focus back and the
 * view posts nothing. The blur first makes the focus a move and not a leftover.
 */
import { expect } from 'vitest';
import { act, fireEvent } from '@testing-library/react';

export async function blurThenSubmitEmpty(input: HTMLInputElement, button: HTMLElement): Promise<void> {
  input.blur();
  expect(document.activeElement).not.toBe(input);
  await act(async () => { fireEvent.click(button); });
  expect(document.activeElement).toBe(input);
}
