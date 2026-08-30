/**
 * One block of text with its math rendered.
 *
 * TWO memos, and both are correctness rather than optimization, because the parent re-renders
 * about once a second while a clock ticks:
 *
 *  1. `useMemo` on the source text keeps KaTeX from a new parse on every tick.
 *  2. The `{ __html }` OBJECT is memoized too, and the component is `memo`'d. Without that,
 *     React compares `dangerouslySetInnerHTML` by identity, sees a fresh object on every
 *     render, and re-applies `innerHTML` unconditionally. The whole KaTeX subtree is then
 *     rebuilt once a second, and a text selection inside the problem dies with it. The 1.0
 *     port measured that as a childList mutation on `.problem-text` at every tick.
 *
 * The class goes on THIS node, not on a wrapper: `app.css` targets `.problem-text .katex`, so
 * the KaTeX output has to be a descendant of the classed element.
 */
import { memo, useMemo } from 'react';
import { renderMathToHtml } from '@/lib/katex';

export interface MathBlockProps {
  /** The source text. Math sits between the delimiter pairs of `lib/katex.ts`. */
  children: string;
  /** The class of the rendered node. It carries the KaTeX cascade. */
  className?: string;
}

export const MathBlock = memo(function MathBlock({
  children,
  className = 'problem-text',
}: MathBlockProps) {
  // React diffs the OBJECT identity, so it must stay stable across renders.
  const markup = useMemo(() => ({ __html: renderMathToHtml(children ?? '') }), [children]);

  // `renderMathToHtml` escapes the source through the `textContent` of a detached node before
  // KaTeX sees it, so model-authored markup arrives here as text. This is not the injection
  // site it resembles — see `lib/katex.ts`.
  return <div className={className} dangerouslySetInnerHTML={markup} />;
});
