/** The closed "Where your progress comes from" disclosure of the status card. */
import { Stat } from '@/components/primitives';
import { num, pct } from '@/lib/format';
import type { StatusResponse } from '@/api/types';

export function ProgressDetails({ status }: { status: StatusResponse }) {
  const mastery = status.mastery;
  const practiced = num(mastery?.practiced);
  const inferred = num(mastery?.inferred);
  const toConfirm = mastery?.to_confirm?.length ?? 0;
  return (
    <details className="progress-more">
      <summary>Where your progress comes from</summary>
      <div className="stat-grid progress-grid">
        <Stat value={`${pct(status.velocity.course_progress)}%`} label="course"
          title="Share of the course's topics you have practiced." />
        <Stat value={`${num(status.nearly_due)}`} label="review soon"
          title="Reviews that come due in the next few days." />
        {mastery ? (
          <>
            <Stat value={`${practiced}`} label="practiced"
              title="Topics where you have answered problems yourself." />
            <Stat value={`${inferred}`} label="skills you showed at the start"
              title="Topics the starting questions showed you know, without practice. One more correct answer confirms each one." />
            <Stat
              value={`${toConfirm}`}
              label="to check again"
              className={toConfirm > 0 ? 'accent' : undefined}
              title="Topics assumed known that still need one correct answer."
            />
          </>
        ) : null}
      </div>
      <ul className="tile-help muted small">
        <li>Course: the share of the course topics you have practiced.</li>
        <li>Review soon: reviews that come due in the next few days.</li>
        {mastery ? (
          <>
            <li>Practiced: topics where you answered problems yourself.</li>
            <li>Skills you showed at the start: topics the starting questions showed you know. One correct answer confirms each one.</li>
            <li>To check again: assumed topics that still need one correct answer.</li>
          </>
        ) : null}
      </ul>
    </details>
  );
}
