/**
 * The answer control of one problem: the choice buttons or the typed field.
 *
 * THE RULE. If the payload has the key `choices` and the list is not empty, the view shows
 * `ChoiceButtons` and no `AnswerField`. If not, the view shows `AnswerField` as before.
 *
 * ONE SUBMIT PATH. The two modes give the same `AnswerFieldHandle` to the view. A tap writes
 * the option text, then calls the `onSubmit` of the typed answer. That submit function reads
 * `value()` synchronously, as it does for a typed answer. Thus the phase rule, the grade
 * reply, the feedback, the quiz receipt and the re-solve flow do not change.
 *
 * `value()` of the choice mode returns the raw option text: no trim and no case change.
 * `clear()` removes the selection, so the re-solve of the same problem starts with no answer
 * and a drill timeout posts a blank answer.
 *
 * `focus()` of the choice mode puts the focus on the container, not on the first button. A
 * held Enter from the Continue button of the previous problem repeats on a focused button
 * and submits an option that the learner did not select. The container ignores that key.
 */
import { useImperativeHandle, useRef, type ReactNode, type Ref } from 'react';
import { AnswerField, type AnswerFieldHandle } from '@/components/AnswerField';
import { ChoiceButtons } from '@/components/ChoiceButtons';

/** True if the payload has one option or more. An absent key and `[]` give the typed field. */
export function hasChoices(choices: string[] | undefined): choices is string[] {
  return choices !== undefined && choices.length > 0;
}

export interface AnswerInputProps {
  /** The key `choices` of the served problem or of the probe. */
  choices: string[] | undefined;
  /** The disabled rule of the typed field. */
  disabled: boolean;
  /** The disabled rule of the buttons: true in each phase that is not `ready`. */
  locked?: boolean;
  onSubmit: () => void;
  onHint?: (() => void) | undefined;
  ref?: Ref<AnswerFieldHandle> | undefined;
}

export function AnswerInput({ choices, disabled, locked = disabled, onSubmit, onHint, ref }: AnswerInputProps) {
  if (hasChoices(choices)) {
    return <ChoiceInput choices={choices} locked={locked} onSubmit={onSubmit} ref={ref} />;
  }
  return <AnswerField ref={ref} disabled={disabled} onSubmit={onSubmit} onHint={onHint} />;
}

/**
 * The primary Submit button of the typed answer.
 *
 * A tap on an answer button is the submit, so a problem with `choices` shows no Submit button.
 */
export function TypedSubmit({ choices, busy, disabled, onClick, children }: {
  choices: string[] | undefined;
  /** True while the grade is in flight. */
  busy: boolean;
  disabled: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  if (hasChoices(choices)) return null;
  return (
    <button
      type="button"
      className={`btn btn-primary${busy ? ' is-busy' : ''}`}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function ChoiceInput({ choices, locked, onSubmit, ref }: {
  choices: string[];
  locked: boolean;
  onSubmit: () => void;
  ref: Ref<AnswerFieldHandle> | undefined;
}) {
  const selected = useRef('');
  const boxRef = useRef<HTMLDivElement>(null);

  useImperativeHandle(ref, () => ({
    value: () => selected.current,
    clear: () => { selected.current = ''; },
    // The container is on screen for as long as the view holds the handle.
    focus: () => { boxRef.current!.focus(); },
  }));

  return (
    <div ref={boxRef} tabIndex={-1} className="choice-input">
      <ChoiceButtons
        choices={choices}
        disabled={locked}
        // Write first, then submit: the submit function reads `value()` before its first await.
        onSubmit={(answer) => { selected.current = answer; onSubmit(); }}
      />
    </div>
  );
}
