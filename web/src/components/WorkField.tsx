/**
 * The collapsible working area.
 *
 * Uncontrolled for the reasons `AnswerField` gives, and a native `<details>` rather than a
 * state-driven div: `app.css` styles `.work summary`, and the native element carries the
 * keyboard and screen-reader semantics for free. A div loses both.
 */
import { useImperativeHandle, useRef, type Ref } from 'react';

export interface WorkFieldHandle {
  /** The trimmed working, read synchronously at submit time. */
  value: () => string;
}

export interface WorkFieldProps {
  onSubmit?: () => void;
  ref?: Ref<WorkFieldHandle>;
}

export function WorkField({ onSubmit, ref }: WorkFieldProps) {
  const areaRef = useRef<HTMLTextAreaElement>(null);

  // No `clear()`. The mandatory `key={problem_id}` on the problem subtree remounts this field
  // at every problem, so nothing needs to clear it. Unused surface on a component this
  // delicate is surface a later unit misuses.
  useImperativeHandle(ref, () => ({
    value: () => areaRef.current?.value.trim() ?? '',
  }), []);

  return (
    <details className="work">
      <summary>Show working (optional)</summary>
      <textarea
        ref={areaRef}
        className="work-input"
        rows={4}
        placeholder="Show your working (optional)…"
        aria-label="Working"
        defaultValue=""
        onKeyDown={(e) => {
          // The same contract as the answer field's Enter: the view's phase gate decides
          // whether a submit starts, not this handler. Shift+Enter stays a newline.
          if (e.key !== 'Enter' || e.shiftKey) return;
          const area = e.currentTarget;
          e.preventDefault();
          if (area.disabled || area.readOnly) return;
          onSubmit?.();
        }}
      />
    </details>
  );
}
