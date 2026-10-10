/** The four tiles of the status card, and the lines that explain a streak or an unmarked answer. */
import { Stat } from '@/components/primitives';
import { num } from '@/lib/format';
import type { StatusResponse } from '@/api/types';

/** The one muted line under the tiles, when the streak needs it. */
function streakNote(streak: number, due: number, today: number): string | null {
  if (streak === 0 && due > 0) return 'Answer one problem today to start a streak.';
  if (streak > 0 && today === 0) {
    return `Answer one problem today to keep your streak at ${streak} ${streak === 1 ? 'day' : 'days'}.`;
  }
  return null;
}

export interface MainTilesProps {
  status: StatusResponse;
  finishBy: string;
  finishTitle: string;
}

export function MainTiles({ status, finishBy, finishTitle }: MainTilesProps) {
  const due = num(status.due_reviews);
  const streak = num(status.xp.streak_days);
  const ungraded = num(status.ungraded);
  const note = streakNote(streak, due, num(status.xp.today));
  const them = ungraded === 1 ? 'this answer' : 'these answers';
  return (
    <>
      <div className="stat-grid stat-grid-main">
        <Stat value={`${streak}`} label="day streak" className="accent"
          title="Days in a row with at least one answered problem." />
        <Stat value={`${due}`} label="due now" className={due > 0 ? 'warn' : undefined}
          title="Reviews to do today. If you skip them, you forget the skill." />
        <Stat value={`${num(status.frontier)}`} label="new topics"
          title="New topics that are ready for you to learn next." />
        <Stat value={finishBy} label="finish by" title={finishTitle} />
      </div>
      {note ? <p className="muted small tile-note">{note}</p> : null}
      {/* D-F2: the attempts nobody graded. The tile appears only when one waits. */}
      {ungraded > 0 ? (
        <>
          <div className="stat-grid stat-grid-main">
            <Stat value={`${ungraded}`} label="not marked" className="warn"
              title={`The checker could not read ${them}: usually a form it cannot read, such as a missing unit. Nothing was scored for or against you, and a person can still mark ${ungraded === 1 ? 'it' : 'them'} by hand.`} />
          </div>
          <p className="muted small tile-note">
            {`Not marked: the checker could not read ${them}. Nothing was scored for or against you.`}
          </p>
        </>
      ) : null}
    </>
  );
}
