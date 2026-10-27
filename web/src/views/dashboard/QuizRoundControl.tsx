/**
 * The timed quiz control (D-QZ1): a length, a scope, and one button.
 *
 * The number buttons set a size. They are not an answer menu. A press on "Continue"
 * starts the session, asks the service for the round, and opens the quiz on its ready card.
 * The clock does not start there. No button here is a primary action (W-C2): the dashboard
 * keeps exactly one `.btn-primary`.
 */
import { useState } from 'react';
import type { Call } from '@/hooks/useCall';
import { useLifetime } from '@/hooks/useLifetime';
import type { ApiClient, PlanTask, QuizRound } from '@/api/types';

const LENGTHS = [5, 10, 20];

export interface QuizRoundControlProps {
  api: ApiClient;
  /** The one request wrapper of the screen (F-36-1). */
  call: Call;
  /** The name of the enrolled course, or null when none stands. */
  courseName: string | null;
  /** Called with the round task, after the service answers. `resumed` is true for a round that already runs. */
  onStarted: (task: PlanTask, round: QuizRound, resumed?: boolean) => void;
}

export function QuizRoundControl({ api, call, courseName, onStarted }: QuizRoundControlProps) {
  const [length, setLength] = useState(10);
  const [scope, setScope] = useState<'course' | 'all'>('course');
  const [busy, setBusy] = useState(false);
  const life = useLifetime();

  const show = async (): Promise<void> => {
    setBusy(true);
    try {
      const started = await call(() => api.sessionStart());
      // A reply that lands after the view left moves nothing (F-F2-2).
      if (!started || !life.alive()) return;
      const res = await call(() => api.startQuizRound(length, scope));
      if (!res || !life.alive()) return;
      if (res.resumed) onStarted(res.task, res.round, true);
      else onStarted(res.task, res.round);
    } finally {
      if (life.alive()) setBusy(false);
    }
  };

  return (
    <div className="quiz-round-control">
      <h3 className="more-heading">Timed quiz</h3>
      <p className="auth-label" id="quiz-length-label">How many questions</p>
      <div className="more-actions" role="group" aria-labelledby="quiz-length-label">
        {LENGTHS.map((value) => (
          <button
            key={value}
            type="button"
            className="btn"
            aria-pressed={value === length}
            onClick={() => { setLength(value); }}
          >
            {value}
          </button>
        ))}
      </div>
      <p className="auth-label" id="quiz-scope-label">Take questions from</p>
      <div className="more-actions" role="group" aria-labelledby="quiz-scope-label">
        <button
          type="button"
          className="btn"
          aria-pressed={scope === 'course'}
          onClick={() => { setScope('course'); }}
        >
          {courseName ?? 'My course'}
        </button>
        <button
          type="button"
          className="btn"
          aria-pressed={scope === 'all'}
          onClick={() => { setScope('all'); }}
        >
          All my courses
        </button>
      </div>
      <div className="more-actions">
        <button
          type="button"
          className="btn"
          disabled={busy}
          title="You see the number of questions and the target time before the clock starts."
          onClick={() => { void show(); }}
        >
          Continue
        </button>
      </div>
    </div>
  );
}
