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
  return (
    <div className="modal picker" role="dialog" aria-modal="true" aria-labelledby="picker-h">
      <h2 id="picker-h">Switch course</h2>
      <p className="muted small">Your progress in every course is kept.</p>
      <div className="picker-list">
        {courses.map((c) => (
          <button
            key={c.id}
            type="button"
            className="btn"
            disabled={c.current}
            onClick={() => onDone(c.id)}
          >
            {c.current ? `${c.name} · current` : c.name}
          </button>
        ))}
      </div>
      <div className="modal-actions">
        <button type="button" className="btn btn-ghost" onClick={() => onDone(null)}>
          Cancel
        </button>
      </div>
    </div>
  );
}
