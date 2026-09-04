/**
 * KaTeX, rendered to a STRING.
 *
 * WHY NOT `renderMathInElement` ON A REACT-OWNED NODE (spec section 4.3, rule 1).
 *
 * `renderMathInElement` mutates the DOM in place: it walks the text nodes and replaces each
 * `$…$` run with a `<span class="katex">` subtree. A vanilla view builds its DOM once, so
 * that is safe there. Under React it is a defect, and a quiet one. The session clock and the
 * quiz clock tick at 1 Hz, so the problem card re-renders about once a second. React then
 * reconciles a subtree whose real DOM no longer matches its virtual DOM. It writes
 * `textContent` back — the math flashes to raw `$\dfrac{1}{2}$` once a second — or it throws
 * `NotFoundError: Failed to execute 'removeChild'` and blanks the page mid-problem. KaTeX
 * also keeps the original TeX in an `<annotation>` element, so a second auto-render pass over
 * rendered output garbles it.
 *
 * The idiom renders to a string, memoizes on the source text, and hands React one opaque
 * payload. A clock tick then costs nothing.
 *
 * THE ESCAPING STEP, which is the security property (spec section 4.3, rule 3).
 *
 * The input is model output. A naive `dangerouslySetInnerHTML={{ __html: text }}` is an XSS
 * hole. The mechanism below is deliberately not a hand-written delimiter splitter with manual
 * escaping. It renders into a DETACHED node, sets `textContent` FIRST — so the DOM itself
 * escapes every character of the source — runs the real `renderMathInElement`, and takes
 * `.innerHTML`. Model-authored markup is never parsed as HTML.
 */

export interface KatexDelimiter {
  left: string;
  right: string;
  display: boolean;
}

/**
 * The four delimiter pairs of spec section 4.3.
 *
 * ORDER IS LOAD-BEARING: `$$` comes before `$`. Reversed, auto-render takes the first `$` of a
 * display block as an inline open, and the display equations render as garbage.
 */
export const DELIMITERS: readonly KatexDelimiter[] = [
  { left: '$$', right: '$$', display: true },
  { left: '\\[', right: '\\]', display: true },
  { left: '$', right: '$', display: false },
  { left: '\\(', right: '\\)', display: false },
];

type AutoRender = (
  root: HTMLElement,
  opts: { delimiters: readonly KatexDelimiter[]; throwOnError: boolean },
) => void;

declare global {
  interface Window {
    /** The vendored `auto-render.min.js` writes it. Absent until that script ran. */
    renderMathInElement?: AutoRender;
  }
}

/**
 * The auto-render extension, read off the window at call time.
 *
 * `index.html` loads it as a UMD global from the vendored tree, `katex.min.js` first (see
 * `vendor-tags.ts`). A read at module scope captures `undefined` when the bundle evaluates
 * before the deferred vendor scripts, and every problem then shows raw LaTeX forever.
 */
function autoRender(): AutoRender | null {
  const fn = window.renderMathInElement;
  return typeof fn === 'function' ? fn : null;
}

/**
 * The rendered HTML of each source string seen so far.
 *
 * KaTeX parses and lays out every expression, and one statement appears in the problem card,
 * in the feedback panel and in the solution. The cache is keyed on the exact source and the
 * function is pure, so it holds no stale entry. `MathBlock` memoizes as well; this second
 * layer survives the remount that `key={problem_id}` forces at every new problem.
 */
const CACHE = new Map<string, string>();

/** The cache holds this many entries. One entry is a few kilobytes of HTML. */
const CACHE_LIMIT = 64;

/**
 * Empty the cache.
 *
 * The test setup calls it before each test. The cache is a module-scope singleton and Vitest
 * isolates modules per FILE, so without this a later test in the same file counts zero KaTeX
 * calls for a string an earlier test already rendered.
 */
export function resetMathCache(): void {
  CACHE.clear();
}

/**
 * Render the `$…$` math in `text` and return HTML that is safe to inject.
 *
 * On a KaTeX failure, and when the vendored scripts are absent, the escaped raw text stays
 * visible. That is the degradation `throwOnError: false` asks for: the learner reads
 * `$\dfrac{1}{2}$` and works on, where a throw gives a blank problem.
 */
export function renderMathToHtml(text: string): string {
  const source = text ?? '';
  const hit = CACHE.get(source);
  if (hit !== undefined) return hit;

  const host = document.createElement('div');

  // `textContent`, never `innerHTML`. THIS is the escaping step: every `<`, `&` and quote in
  // the model's output becomes a text node before anything parses it as markup.
  host.textContent = source;

  const render = autoRender();
  if (render) {
    try {
      render(host, { delimiters: DELIMITERS, throwOnError: false });
    } catch {
      // Put the escaped source back, rather than throw into a React render.
      host.textContent = source;
    }
  }

  const html = host.innerHTML;
  if (CACHE.size >= CACHE_LIMIT) {
    const oldest = CACHE.keys().next().value;
    if (oldest !== undefined) CACHE.delete(oldest);
  }
  CACHE.set(source, html);
  return html;
}
