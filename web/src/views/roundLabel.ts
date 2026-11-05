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

/** The part of an exam, as words. */
export function partWords(part: 1 | 2): string {
  return part === 1 ? 'part I' : 'part II';
}

/** The name of an exam set with its part: "Exam set 1: 2025 main paper, part I". */
export function setWithPart(name: string | null, id: string, part?: 1 | 2): string {
  const shown = name ?? id;
  if (part === undefined) return shown;
  return `${shown.replace(/,?\s*part\s+II?\s*$/i, '')}, ${partWords(part)}`;
}

/** What one exam set shows before its button: not started, the points so far, or done. */
export function setStateLine(points: number | null | undefined, max?: number, target?: number): string {
  if (points === null || points === undefined || max === undefined) return 'Not started yet.';
  if (target !== undefined && points >= target) return `Last paper: ${points} of ${max} points. Done.`;
  const aim = target === undefined ? '' : ` Aim for ${target} or more.`;
  return `Last paper: ${points} of ${max} points.${aim}`;
}

/** The label of the button that starts the paper of one exam set. */
export function startSetLabel(name: string | null, id: string, part?: 1 | 2): string {
  const shown = setWithPart(name, id, part);
  return /^exam set/i.test(shown) ? `Start ${shown}` : `Start exam set ${shown}`;
}
