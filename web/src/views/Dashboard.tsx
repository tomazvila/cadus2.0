/**
 * The dashboard — the landing screen, and the port of `static/views/dashboard.js`.
 *
 * It reads ONE route, `GET /api/status`, and it decides nothing. The core owns the
 * schedule (Hard Rule 3), so this screen counts what the core reports and offers the way
 * in. It never composes a plan of its own.
 *
 * THREE SHAPE RULES, and each one is a pinned invariant.
 *
 *   W-C2 — ONE primary action. Every state of this screen renders exactly one
 *   `.btn-primary`. A screen with three equal buttons asks the learner to plan the study
 *   session, which is the job the scheduler already did.
 *
 *   W-C3 — NO DEAD ENDS. An empty plan still offers the diagnostic. "Empty plan" is a
 *   status with no due review, no nearly-due review, no frontier lesson, no quiz and no
 *   drill — `hasScheduledWork()` below. The learner who finishes everything, and the
 *   learner who never placed, both get one action that moves them forward.
 *
 *   W-C5 — QUIET SECONDARIES. Everything else sits under a native `<details>`. A native
 *   disclosure needs no ARIA, keeps its keyboard behavior, and prints open.
 *
 * DEP-3 — THE EXPORT. "Export my data (JSONL)" streams the full append-only event log
 * through `GET /api/export`. It goes through `fetch` with the HttpOnly session cookie and
 * carries NO token: no query parameter, no header, nothing in `localStorage` (SEC-cookie).
 *
 * WHY THE EXPORT BYPASSES `useCall`. `downloadExport()` resolves `void`, and the return
 * contract of `useCall` is "a failure resolves undefined". A `void` success is
 * indistinguishable from a failure there, so this one call handles its own error and
 * toasts it.
 *
 * WHY A REDUCER, not a `setState` per field. A Retry from `useCall` re-runs the
 * continuation many renders later (F-36-1). A continuation that closed over render state
 * would write stale data; `dispatch` and the captured generation NUMBER stay correct.
 *
 * WHAT THIS UNIT DOES NOT OWN. There is no router yet, so the four navigation callbacks
 * are props. The unit that adds URL routing (spec section 4.1) supplies the real ones.
 */
import { Fragment, useCallback, useEffect, useReducer } from 'react';
import { useDialogs } from '@/components/Modal';
import { LoadingBlock, Ring, Stat } from '@/components/primitives';
import { useBusy } from '@/hooks/useBusy';
import { useCall } from '@/hooks/useCall';
import { useLifetime } from '@/hooks/useLifetime';
import { num, pct } from '@/lib/format';
import { toast } from '@/app/toast';
import type { ApiClient, JourneyCourse, PlanTask, StatusResponse } from '@/api/types';

export interface DashboardProps {
  api: ApiClient;
  /** Demo mode. A 401 then keeps the learner on the screen. */
  demo?: boolean;
  /** The session-expired path of `useCall`. */
  onUnauthorized: () => void;
  /** Go to the study loop, after `POST /api/session/start` answers. */
  onSession: () => void;
  /** Go to the quiz, WITH the plan task: the clock reads the task budget (QUIZ-budget). */
  onQuiz: (task: PlanTask) => void;
  /** Go to the placement diagnostic. */
  onDiagnostic: () => void;
  /** Go to the curriculum map. */
  onMap: () => void;
}

/**
 * Does the core schedule anything for this learner right now?
 *
 * False is the "empty plan" of W-C3. Every counter reads through `num()`, because one
 * absent key must not make an empty plan look full.
 */
export function hasScheduledWork(status: StatusResponse): boolean {
  return (
    num(status.due_reviews) > 0
    || num(status.nearly_due) > 0
    || num(status.frontier) > 0
    || status.quiz_due === true
    || status.drill_due === true
  );
}

interface State {
  /** Bumped by anything that changes server state. The fetch effect depends on it. */
  gen: number;
  status: StatusResponse | null;
  loadedGen: number;
  failedGen: number;
}

type Action =
  | { type: 'reload' }
  | { type: 'ok'; gen: number; status: StatusResponse }
  | { type: 'fail'; gen: number };

const INITIAL: State = { gen: 0, status: null, loadedGen: -1, failedGen: -1 };

/**
 * The generation guard, and why it earns its lines: two clicks in separate ticks put two
 * `GET /api/status` calls in flight. Without the comparison the older reply lands last and
 * overwrites fresh data, or fails last and replaces a good card with "Could not load".
 */
function reduce(state: State, action: Action): State {
  switch (action.type) {
    case 'reload':
      return { ...state, gen: state.gen + 1 };
    case 'ok':
      return action.gen < state.loadedGen
        ? state
        : { ...state, status: action.status, loadedGen: action.gen };
    case 'fail':
      return action.gen < state.failedGen ? state : { ...state, failedGen: action.gen };
  }
}

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
function CoursePicker({
  courses,
  onDone,
}: {
  courses: JourneyCourse[];
  onDone: (id: string | null) => void;
}) {
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

export function Dashboard({
  api,
  demo = false,
  onUnauthorized,
  onSession,
  onQuiz,
  onDiagnostic,
  onMap,
}: DashboardProps) {
  const life = useLifetime();
  const dialogs = useDialogs();
  const busy = useBusy();
  const call = useCall({ demo, onUnauthorized });
  const [state, dispatch] = useReducer(reduce, INITIAL);

  const gen = state.gen;
  useEffect(() => {
    void call(
      () => api.getStatus(),
      (status) => {
        if (life.alive()) dispatch({ type: 'ok', gen, status });
      },
    ).then((status) => {
      if (!status && life.alive()) dispatch({ type: 'fail', gen });
    });
  }, [api, call, life, gen]);

  const reload = useCallback(() => dispatch({ type: 'reload' }), []);

  const startSession = useCallback(async () => {
    const started = await call(() => api.sessionStart());
    // F-F2-2: this navigation lands after an await, so the view must still own the screen.
    // Without the guard a slow `/session/start` pulls the learner out of a screen they
    // opened in the meantime.
    if (!started || !life.alive()) return;
    onSession();
  }, [api, call, life, onSession]);

  const doEnroll = useCallback(
    async (course: JourneyCourse) => {
      const res = await call(() => api.enroll(course.id));
      if (!res || !life.alive()) return;
      toast(`Enrolled in ${course.name}. Take the placement to get started.`, { kind: 'success' });
      reload();
    },
    [api, call, life, reload],
  );

  const quizNow = useCallback(async () => {
    const started = await call(() => api.sessionStart());
    if (!started || !life.alive()) return;
    const plan = await call(() => api.getPlan());
    if (!plan || !life.alive()) return;
    const quiz = plan.tasks.find((t) => t.task_type === 'quiz');
    if (!quiz) {
      toast('No quiz is due right now.', { kind: 'info' });
      return;
    }
    // WITH the task, never the id alone: the quiz clock reads `time_budget_secs` of the
    // task, and a serve value is one question's expected time (QUIZ-budget).
    onQuiz(quiz);
  }, [api, call, life, onQuiz]);

  const switchCourse = useCallback(
    async (courses: JourneyCourse[]) => {
      const choice = await dialogs.open<string>((resolve) => (
        <CoursePicker courses={courses} onDone={resolve} />
      ));
      const course = courses.find((c) => c.id === choice);
      if (course) await doEnroll(course);
    },
    [dialogs, doEnroll],
  );

  /** DEP-3. Read-only, through the cookie. See the module note above. */
  const exportData = useCallback(async () => {
    try {
      await api.downloadExport();
    } catch (e) {
      toast((e as { message?: string } | null)?.message || 'Could not export your data.', {
        kind: 'error',
      });
    }
  }, [api]);

  // The error screen shows only while the NEWEST attempt is the failed one. The Retry of
  // the toast — the recovery path `useCall` offers — then clears it by succeeding, instead
  // of leaving the view with good data and refusing to render it.
  const failed = state.failedGen > state.loadedGen;
  const status = state.status;

  if (failed && !status) {
    return (
      <section className="view-dashboard">
        <div className="empty">
          <p>Could not load your dashboard.</p>
          <button type="button" className="btn btn-primary" onClick={reload}>
            Try again
          </button>
        </div>
      </section>
    );
  }

  if (!status) {
    return (
      <section className="view-dashboard">
        <LoadingBlock label="Loading your dashboard…" />
      </section>
    );
  }

  const courses = status.courses;
  const today = num(status.xp.today);
  const goal = num(status.xp.goal, 40);
  const fraction = goal ? today / goal : 0;
  const due = num(status.due_reviews);
  const frontier = num(status.frontier);

  const courseArc = courses.length ? (
    <div className="course-arc">
      {courses.map((c, i) => (
        // A Fragment, not a wrapper span: `.course-arc` is a flex row with a gap, and a
        // wrapper makes each pair ONE flex child. The gap then lands only before each
        // separator, and a wrap breaks a course away from its own ▸.
        <Fragment key={c.id}>
          {i ? <span className="arc-sep" aria-hidden="true">▸</span> : null}
          <span className={c.current ? 'arc-course arc-current' : 'arc-course'}>{c.name}</span>
        </Fragment>
      ))}
    </div>
  ) : null;

  // W-C3, first shape: a learner with no placement gets ONE onboarding action.
  if (status.placed === false) {
    return (
      <section className="view-dashboard">
        {courseArc}
        <div className="card onboard-card">
          <h2>Let&apos;s find where to start.</h2>
          <p className="muted">
            A short placement — up to 40 questions, about 2 minutes. Stop at any time.
          </p>
          <button type="button" className="btn btn-primary btn-hero" onClick={onDiagnostic}>
            Start placement <span aria-hidden="true">▸</span>
          </button>
          <p className="muted onboard-foot">Nothing else to decide — we take it from here.</p>
        </div>
      </section>
    );
  }

  const work = hasScheduledWork(status);
  const bits: string[] = [];
  if (due) bits.push(`${due} review${due === 1 ? '' : 's'}`);
  if (frontier) bits.push(`${frontier} new lesson${frontier === 1 ? '' : 's'}`);
  if (status.quiz_due) bits.push('a quiz');
  if (status.drill_due) bits.push('a drill');

  const currentIndex = courses.findIndex((c) => c.current);
  const nextCourse =
    currentIndex >= 0 && currentIndex + 1 < courses.length ? courses[currentIndex + 1] : null;

  return (
    <section className="view-dashboard">
      {courseArc}

      <div className="card status-card">
        <div className="status-head">
          <div>
            <h2>{`${today} / ${goal} XP today`}</h2>
            <p className="muted">
              {`${status.course.name ?? 'your course'} · ${pct(status.velocity.course_progress)}% complete`}
            </p>
          </div>
          <Ring fraction={fraction} label={`${pct(fraction)}%`} sub="daily goal" />
        </div>
        <div className="stat-grid">
          <Stat value={`${num(status.xp.streak_days)}`} label="day streak" className="accent" />
          <Stat value={`${due}`} label="due now" className={due > 0 ? 'warn' : ''} />
          <Stat value={`${num(status.nearly_due)}`} label="nearly due" />
          <Stat value={`${frontier}`} label="frontier" />
          <Stat value={`${pct(status.velocity.course_progress)}%`} label="course" />
          <Stat value={status.velocity.eta ?? '—'} label="ETA" />
        </div>
      </div>

      {/* W-C2: one primary action, chosen by the state of the plan. */}
      <div className="primary-action">
        {work ? (
          <>
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
          </>
        ) : (
          <>
            <p className="caught-up">You are all caught up — nice work.</p>
            {nextCourse ? (
              <>
                <button
                  type="button"
                  className={busy.cls('enroll', 'btn btn-primary btn-hero')}
                  disabled={busy.is('enroll')}
                  onClick={() => busy.run('enroll', () => doEnroll(nextCourse))}
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
          </>
        )}
      </div>

      {/* W-C5: everything else is quiet, under a native disclosure. */}
      <details className="more-menu">
        <summary>More</summary>
        <div className="more-actions">
          <button type="button" className="btn" onClick={onMap}>
            Curriculum map
          </button>
          <button
            type="button"
            className={busy.cls('quiz', 'btn')}
            disabled={busy.is('quiz')}
            onClick={() => busy.run('quiz', quizNow)}
          >
            Quiz now
          </button>
          <button type="button" className="btn" onClick={onDiagnostic}>
            Re-run the placement
          </button>
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
            Export my data (JSONL)
          </button>
        </div>
        <p className="muted small more-caption">
          Export my data downloads your full event log — the same data a re-import consumes.
        </p>
      </details>

      {demo ? (
        <p className="demo-hint muted small">
          Demo mode — every button works against canned data, with no backend.
        </p>
      ) : null}
    </section>
  );
}
