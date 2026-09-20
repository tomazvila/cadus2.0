/**
 * The answer buttons of a Label item (freeze pack, `rust-api.md` section 5).
 *
 * The service sends the display text of each option in the key `choices`. The order is the
 * order of the service: it is stable for one `problem_id`, so a reload shows the same order.
 * This component does not sort and does not shuffle.
 *
 * A tap gives the option text to `onSubmit` with no change: no trim and no case change. The
 * text with `$…$` renders as math, and the submitted value is the raw text with its `$`.
 *
 * THE MARK. After a tap the buttons are locked, and the button of the submitted option has
 * the class `is-selected` and `aria-pressed="true"`. Thus the verdict view shows the answer
 * that the app sent, as the locked typed field does for a typed answer. An enabled button has
 * no mark.
 *
 * Each option is a real `<button type="button">`. The DOM order is the visual order, so the
 * keyboard focus order is the visual order too.
 */
import { MathBlock } from '@/components/MathBlock';

export interface ChoiceButtonsProps {
  /** The display text of each option, in the order to show. */
  choices: string[];
  /** True while a grade is in flight and after the verdict. */
  disabled: boolean;
  /** Receives the raw text of the option that the learner selected. */
  onSubmit: (answer: string) => void;
  /** The index of the submitted option, or null. The owner of the selection gives it. */
  selected?: number | null;
  /** Receives the index of the option, before `onSubmit`. */
  onSelect?: (index: number) => void;
}

export function ChoiceButtons({ choices, disabled, onSubmit, selected = null, onSelect }: ChoiceButtonsProps) {
  return (
    <div className="choice-buttons" role="group" aria-label="Answer choices">
      {choices.map((choice, index) => {
        // The mark shows only on a locked button: an enabled button is not an answer yet.
        const marked = disabled && index === selected;
        return (
          <button
            // The index is part of the key: two options with the same text stay two buttons.
            key={`${index}:${choice}`}
            type="button"
            className={`btn choice-button${marked ? ' is-selected' : ''}`}
            aria-pressed={marked ? true : undefined}
            disabled={disabled}
            onClick={() => { onSelect?.(index); onSubmit(choice); }}
          >
            <MathBlock as="span" className="choice-text">{choice}</MathBlock>
          </button>
        );
      })}
    </div>
  );
}
