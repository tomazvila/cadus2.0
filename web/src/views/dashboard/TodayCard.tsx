/**
 * The Today card (D-DP1): the date, the study-day number, the set of the day, one read, and
 * the pace of the week.
 *
 * The card reads `status.today` and decides nothing. The set is the plan the service
 * composed, frozen at the first session start of the local day. The card adds no primary
 * action (W-C2): the read button is a plain `.btn`.
 */
import type { StatusResponse, TodayItem } from '@/api/types';

export interface TodayCardProps {
  status: StatusResponse;
  /** Open the page of one topic. */
  onRead: (topicId: string, name: string) => void;
}

const KINDS: Record<string, string> = {
  lesson: 'Lesson',
  review: 'Review',
  drill: 'Practice drill',
  quiz: 'Timed quiz',
  'multi-step': 'Applied problem',
};

/** "about 5 minutes" from a budget in seconds, or an empty text when there is none. */
function aboutMinutes(secs: number | null): string {
  if (typeof secs !== 'number' || !(secs > 0)) return '';
  const minutes = Math.ceil(secs / 60);
  return `about ${minutes} ${minutes === 1 ? 'minute' : 'minutes'}`;
}

/** "Tuesday, October 27, 2026" from a local day. An unreadable day stays as it is. */
function dateInWords(date: string): string {
  const day = new Date(`${date}T00:00:00`);
  if (Number.isNaN(day.getTime())) return date;
  return day.toLocaleDateString('en-US', {
    weekday: 'long', month: 'long', day: 'numeric', year: 'numeric',
  });
}

function itemLine(item: TodayItem): string {
  const parts = [
    KINDS[item.task_type] ?? item.task_type,
    item.topic ? (item.topic.name ?? item.topic.id) : '',
    aboutMinutes(item.time_budget_secs),
  ].filter((part) => part !== '');
  if (item.progress.done) parts.push('Done');
  else if (item.progress.answered > 0) parts.push(`${item.progress.answered} answered`);
  return parts.join(' · ');
}

export function TodayCard({ status, onRead }: TodayCardProps) {
  const today = status.today;
  if (!today) return null;
  const { read, week } = today;
  const readName = read ? (read.topic.name ?? read.topic.id) : '';
  const days = week.study_days === 1 ? 'day' : 'days';
  const topics = week.topics_passed === 1 ? 'topic' : 'topics';
  return (
    <section className="card today-card">
      <h2>{`Today · Study day ${today.day_number}`}</h2>
      <p className="muted">{dateInWords(today.date)}</p>
      {today.set.length === 0 ? (
        <p>Nothing is due today.</p>
      ) : (
        <ul className="today-set">
          {today.set.map((item, i) => (
            <li key={`${item.task_id}-${i}`}>{itemLine(item)}</li>
          ))}
        </ul>
      )}
      {read ? (
        <div className="today-read">
          <p>{`Read: ${readName}`}</p>
          <button type="button" className="btn" onClick={() => { onRead(read.topic.id, readName); }}>
            {`Read the lesson text: ${readName}`}
          </button>
        </div>
      ) : null}
      <p className="muted">
        {`This week: ${week.study_days} study ${days}, ${week.topics_passed} ${topics} passed.`}
      </p>
    </section>
  );
}
