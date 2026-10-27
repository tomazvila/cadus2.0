/** The closed "Read more about this course" fold: the reading list of a course (D-RL1). */
import type { ReadingEntry } from '@/api/types';
import '../styles/fix-reading.css';

/** The SPA does its own scheme check: only an https address becomes a link. */
function isAddress(where: string): boolean {
  return where.startsWith('https://');
}

export function ReadingFold({ reading }: { reading: ReadingEntry[] | undefined }) {
  if (!reading || reading.length === 0) return null;
  return (
    <details className="reading-fold">
      <summary>Read more about this course</summary>
      <ul>
        {reading.map((entry) => (
          <li key={`${entry.title}|${entry.author}`}>
            <p className="reading-title">
              {`${entry.title}, ${entry.author} `}
              <span className="reading-tag">{entry.access === 'free' ? 'Free to read' : 'Book'}</span>
            </p>
            <p className="muted reading-note">{entry.note}</p>
            {isAddress(entry.where) ? (
              <a href={entry.where} target="_blank" rel="noopener noreferrer">
                {`Open ${entry.title} in a new tab`}
              </a>
            ) : (
              <p className="reading-where">{`Where to read: ${entry.where}`}</p>
            )}
          </li>
        ))}
      </ul>
    </details>
  );
}
