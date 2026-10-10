/** The course path: the previous, current and next course. "Switch course" lists them all. */
import type { JourneyCourse } from '@/api/types';

export function CourseArc({ courses }: { courses: JourneyCourse[] }) {
  const currentAt = courses.findIndex((c) => c.current);
  const shown = currentAt < 0
    ? courses.slice(0, 3)
    : courses.slice(Math.max(0, currentAt - 1), currentAt + 2);
  if (!shown.length) return null;
  return (
    <ol className="course-arc" aria-label="Course path">
      {shown.map((c) => (
        <li
          key={c.id}
          className={c.current ? 'arc-course arc-current' : 'arc-course'}
          aria-current={c.current ? 'step' : undefined}
        >
          {c.name}
        </li>
      ))}
    </ol>
  );
}
