/**
 * The name of a topic or a knowledge point, with its `$…$` math rendered.
 *
 * A name that holds no `$` renders as plain text. A name that holds `$` goes through the
 * same renderer as the problem text (`MathBlock`, `lib/katex.ts`). The result is a `span`,
 * so it sits inside a heading, a button or a sentence.
 */
import { MathBlock } from './MathBlock';

export interface MathNameProps {
  /** The name. `undefined` and the empty string render nothing. */
  children: string | null | undefined;
}

/** True when the name holds a math mark that the renderer reads. */
export function hasMath(name: string): boolean {
  return name.includes('$') || name.includes('\\(') || name.includes('\\[');
}

/**
 * The name as plain text, for a place that takes a string only (a toast, an `aria-label`, the
 * document title). The `$` marks go. The math source stays readable.
 */
export function plainName(name: string | null | undefined): string {
  return (name ?? '').replace(/\$+/g, '');
}

export function MathName({ children }: MathNameProps) {
  if (!children) return null;
  if (!hasMath(children)) return <>{children}</>;
  return (
    <MathBlock as="span" className="name-math">
      {children}
    </MathBlock>
  );
}
