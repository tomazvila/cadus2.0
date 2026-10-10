/**
 * The picker behind "Switch course". It resolves a course id, or null on a cancel.
 *
 * IT IS THE DIALOG SURFACE, so it carries what a dialog carries (spec section 4.5). `Modal`
 * portals this element straight into `.modal-overlay` and adds no wrapper of its own, so
 * `role="dialog"` and `aria-modal="true"` live HERE or nowhere: without them the overlay
 * traps focus in a group a screen reader still reads as part of the page behind it. The
 * `.modal` class is the one rule in `app.css` that paints a dialog surface — the background,
 * the border, the radius, the padding, the width and the `display: grid` the picker's own
 * rows are laid out by — and `.picker` alone paints nothing at all.
 */
import type { JourneyCourse } from '@/api/types';

export interface CoursePickerProps {
  courses: JourneyCourse[];
  onDone: (id: string | null) => void;
}

export function CoursePicker({ courses, onDone }: CoursePickerProps) {
  // The current course first, then the next three. The rest wait under "All courses".
  const at = courses.findIndex((c) => c.current);
  const head = at < 0 ? courses.slice(0, 4) : [courses[at]!, ...courses.slice(at + 1, at + 4)];
  const rest = courses.filter((c) => !head.includes(c));
  const button = (c: JourneyCourse) => (
    <button
      key={c.id}
      type="button"
      className="btn"
      disabled={c.current}
      onClick={() => onDone(c.id)}
    >
      {c.current ? `${c.name} · current` : c.name}
    </button>
  );
  return (
    <div className="modal picker" role="dialog" aria-modal="true" aria-labelledby="picker-h">
      <h2 id="picker-h">Switch course</h2>
      <p className="muted small">Your progress in every course is kept.</p>
      <div className="picker-list">{head.map(button)}</div>
      {rest.length ? (
        <details className="picker-all">
          <summary>All courses</summary>
          <div className="picker-list">{rest.map(button)}</div>
        </details>
      ) : null}
      <div className="modal-actions">
        <button type="button" className="btn btn-ghost" onClick={() => onDone(null)}>
          Cancel
        </button>
      </div>
    </div>
  );
}
