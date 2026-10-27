/** The quiet "More" disclosure (W-C5): the study extras, then the account actions. */
import type { Busy } from '@/hooks/useBusy';
import type { Call } from '@/hooks/useCall';
import type { ApiClient, JourneyCourse, PlanTask, QuizRound, XpState } from '@/api/types';
import { GoalControl } from './GoalControl';
import { QuizRoundControl } from './QuizRoundControl';

export interface MoreMenuProps {
  api: ApiClient;
  call: Call;
  goal: number;
  onGoalSaved: (xp: XpState) => void;
  busy: Busy;
  courses: JourneyCourse[];
  onMap: () => void;
  onProofs: () => void;
  onDiagnostic: () => void;
  courseName: string | null;
  onQuizRound: (task: PlanTask, round: QuizRound, resumed?: boolean) => void;
  switchCourse: (courses: JourneyCourse[]) => Promise<void>;
  exportData: () => Promise<void>;
}

export function MoreMenu({
  api, call, goal, onGoalSaved, busy, courses, onMap, onProofs, onDiagnostic, courseName, onQuizRound, switchCourse, exportData,
}: MoreMenuProps) {
  return (
    <details className="more-menu">
      <summary>More: goal, map, proofs, quiz, data</summary>
      <div className="more-actions">
        <button type="button" className="btn" onClick={onMap}>
          Map
        </button>
        <button type="button" className="btn" onClick={onProofs}>
          Your proofs
        </button>
        <button type="button" className="btn" onClick={onDiagnostic}>
          Starting questions again
        </button>
      </div>
      <QuizRoundControl api={api} call={call} courseName={courseName} onStarted={onQuizRound} />
      <h3 className="more-heading">Your daily goal</h3>
      <GoalControl api={api} call={call} goal={goal} onSaved={onGoalSaved} />
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
