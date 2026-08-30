/**
 * KaTeX ships no types for its auto-render entry point.
 *
 * Only the math test imports it, to run the REAL renderer in place of the recording stub of
 * `setup.ts`. Production reaches the same function through the UMD global that `index.html`
 * loads from the vendored tree (see `src/lib/katex.ts`), so this declaration is a test-only
 * concern.
 */
declare module 'katex/contrib/auto-render' {
  interface AutoRenderDelimiter {
    left: string;
    right: string;
    display: boolean;
  }

  interface AutoRenderOptions {
    delimiters?: readonly AutoRenderDelimiter[];
    throwOnError?: boolean;
    ignoredTags?: readonly string[];
    errorCallback?: (msg: string, err: Error) => void;
  }

  const renderMathInElement: (root: HTMLElement, options?: AutoRenderOptions) => void;
  export default renderMathInElement;
}
