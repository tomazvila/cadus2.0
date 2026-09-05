/**
 * The one primary action of the dashboard (W-C2), chosen by the state of the plan.
 *
 *   work scheduled      "Continue studying", with the line that names the work;
 *   nothing, next course "Start <course>", with the diagnostic beside it (W-C3);
 *   nothing, last course the diagnostic IS the primary action (W-C3).
 *
 * Every branch renders exactly one `.btn-primary`. A screen that ended the last branch with
 * prose alone is the dead end W-C3 exists to forbid.
 */
import { num } from '@/lib/format';
import type { Busy } from '@/hooks/useBusy';
import type { JourneyCourse, StatusResponse } from '@/api/types';

/** The pieces of "Up next: …", in the order the core reports them. */
function upNext(status: StatusResponse): string[] {
  const due = num(status.due_reviews);
  const frontier = num(status.frontier);
  const bits: string[] = [];
  if (due) bits.push(`${due} review${due === 1 ? '' : 's'}`);
  if (frontier) bits.push(`${frontier} new lesson${frontier === 1 ? '' : 's'}`);
  if (status.quiz_due) bits.push('a quiz');
  if (status.drill_due) bits.push('a drill');
  return bits;
}

/** The course after the current one, or null on the last course. */
function nextCourseOf(courses: JourneyCourse[]): JourneyCourse | null {
  const currentIndex = courses.findIndex((c) => c.current);
  // Past the last course, and with no current one, there is nothing after.
  return currentIndex < 0 ? null : courses[currentIndex + 1] ?? null;
}

export interface PrimaryActionProps {
  status: StatusResponse;
  /** Whether the core schedules anything right now. */
  work: boolean;
  busy: Busy;
  startSession: () => Promise<void>;
  enroll: (course: JourneyCourse) => Promise<void>;
  onDiagnostic: () => void;
}

export function PrimaryAction({
  status, work, busy, startSession, enroll, onDiagnostic,
}: PrimaryActionProps) {
  if (work) {
    const bits = upNext(status);
    return (
      <div className="primary-action">
        <button
          type="button"
          className={busy.cls('session', 'btn btn-primary btn-hero')}
          disabled={busy.is('session')}
          onClick={() => busy.run('session', startSession)}
        >
          <span aria-hidden="true">▶</span> Continue studying
        </button>
        <p className="muted primary-sub">
          {bits.length ? `Up next: ${bits.join(' · ')}.` : 'Practice is ready.'}
        </p>
      </div>
    );
  }

  const nextCourse = nextCourseOf(status.courses);
  return (
    <div className="primary-action">
      <p className="caught-up">You are all caught up — nice work.</p>
      {nextCourse ? (
        <>
          <button
            type="button"
            className={busy.cls('enroll', 'btn btn-primary btn-hero')}
            disabled={busy.is('enroll')}
            onClick={() => busy.run('enroll', () => enroll(nextCourse))}
          >
            {`Start ${nextCourse.name} `}
            <span aria-hidden="true">▸</span>
          </button>
          {/* W-C3: the diagnostic stays in the open, beside the primary — never
              only inside the closed disclosure below. */}
          <button type="button" className="btn btn-ghost" onClick={onDiagnostic}>
            Re-check where you are
          </button>
        </>
      ) : (
        // W-C3, the empty plan with nowhere else to go: the diagnostic IS the
        // primary action. A screen that ends here with prose alone is the dead end
        // this invariant exists to forbid.
        <>
          <button type="button" className="btn btn-primary btn-hero" onClick={onDiagnostic}>
            Re-check where you are <span aria-hidden="true">▸</span>
          </button>
          <p className="muted primary-sub">
            A short placement finds the next thing worth your time.
          </p>
        </>
      )}
    </div>
  );
}
