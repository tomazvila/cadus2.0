/**
 * The optional parts of a teach page (D-TM1, D-TD1): the word list, and the folds for the
 * picture, the limit and the second example. A page from before the fields holds none of
 * them, so every part renders nothing for an absent value.
 */
import type { ReactNode } from 'react';
import { MathBlock } from '@/components/MathBlock';

/** The words of the page, each with one plain sentence. Nothing renders for an empty list. */
export function TermList({ terms }: { terms: { term: string; meaning: string }[] | undefined }) {
  if (!terms || terms.length === 0) return null;
  return (
    <div className="teach-terms">
      <div className="teach-label">Words used here</div>
      <dl className="teach-term-list">
        {terms.map((row) => (
          <div key={row.term} className="teach-term">
            <dt className="teach-term-word">{row.term}</dt>
            <dd className="teach-term-meaning">
              <MathBlock as="span">{row.meaning}</MathBlock>
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

/** One fold of the page. It starts open, unless `closed` is set. A learner who knows the part closes it. */
export function DepthFold({ label, closed, children }: { label: string; closed?: boolean; children: ReactNode }) {
  return (
    <details className="teach-depth" open={!closed}>
      <summary>{label}</summary>
      {children}
    </details>
  );
}

/** The picture of the idea (open) above the concept. */
export function PictureFold({ picture }: { picture: string | undefined }) {
  if (!picture) return null;
  return (
    <DepthFold label="An everyday picture">
      <MathBlock className="teach-picture">{picture}</MathBlock>
    </DepthFold>
  );
}

/** The limit of the method (open) under the steps. */
export function WatchOutFold({ watchOut }: { watchOut: string | undefined }) {
  if (!watchOut) return null;
  return (
    <DepthFold label="Where the method fails">
      <MathBlock className="teach-watch-out">{watchOut}</MathBlock>
    </DepthFold>
  );
}
