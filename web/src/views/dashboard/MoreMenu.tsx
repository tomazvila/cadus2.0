/** The quiet "More" disclosure (W-C5): the study extras, then the account actions. */
import type { Busy } from '@/hooks/useBusy';
import type { JourneyCourse } from '@/api/types';

export interface MoreMenuProps {
  busy: Busy;
  courses: JourneyCourse[];
  onMap: () => void;
  onProofs: () => void;
  onDiagnostic: () => void;
  quizNow: () => Promise<void>;
  switchCourse: (courses: JourneyCourse[]) => Promise<void>;
  exportData: () => Promise<void>;
}

export function MoreMenu({
  busy, courses, onMap, onProofs, onDiagnostic, quizNow, switchCourse, exportData,
}: MoreMenuProps) {
  return (
    <details className="more-menu">
      <summary>More: map, proofs, quiz, data</summary>
      <div className="more-actions">
        <button type="button" className="btn" onClick={onMap}>
          Map
        </button>
        <button type="button" className="btn" onClick={onProofs}>
          Your proofs
        </button>
        <button
          type="button"
          className={busy.cls('quiz', 'btn')}
          disabled={busy.is('quiz')}
          title="A timer starts at once. You see no feedback until the end."
          onClick={() => busy.run('quiz', quizNow)}
        >
          Start a timed quiz
        </button>
        <button type="button" className="btn" onClick={onDiagnostic}>
          Starting questions again
        </button>
      </div>
      <h3 className="more-heading">Your account</h3>
      <div className="more-actions">
        <button
          type="button"
          className={busy.cls('switch', 'btn')}
          disabled={busy.is('switch') || courses.length < 2}
          onClick={() => busy.run('switch', () => switchCourse(courses))}
        >
          Switch course
        </button>
        <button
          type="button"
          className={busy.cls('export', 'btn')}
          disabled={busy.is('export')}
          onClick={() => busy.run('export', exportData)}
        >
          Download my data
        </button>
      </div>
      <p className="muted small more-caption">
        Download my data saves a file with every answer you have given.
      </p>
    </details>
  );
}
