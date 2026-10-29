/** The plain words that name where a quiz round takes its questions from (D-QZ1, D-VBE3). */
import type { QuizRound } from '@/api/types';

type Place = Pick<QuizRound, 'scope' | 'course_name'> & { set_name?: string | null | undefined; form?: string | null | undefined };

/** The place of a round, for the ready card and the result heading. */
export function roundPlace(round: Place): string {
  if (round.scope === 'all') return 'all my courses';
  if (round.scope === 'paper') return round.set_name ?? round.form ?? 'this exam set';
  const course = round.course_name ?? 'my course';
  return round.scope === 'course_and_prereqs' ? `${course} and the skills it builds on` : course;
}

/** The score line of a paper: the points earned out of the points asked, and the percent. */
export function paperScoreLine(earned: number, total: number, percent: number): string {
  return `${earned} of ${total} points (${percent}%)`;
}

/** The label of the button that starts the paper of one exam set. */
export function startSetLabel(name: string | null, id: string): string {
  const shown = name ?? id;
  return /^exam set/i.test(shown) ? `Start ${shown}` : `Start exam set ${shown}`;
}
