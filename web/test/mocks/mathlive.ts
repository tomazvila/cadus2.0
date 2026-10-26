/**
 * A MathLive double.
 *
 * `vitest.config.ts` aliases `/vendor/mathlive/mathlive.min.mjs` to this file. It defines the
 * `math-field` custom element with a stored LaTeX string and the members that
 * `lib/math-editor-loader.ts` declares. It does not typeset anything: jsdom has no layout.
 *
 * `test/editor.test.tsx` installs this module through `resetMathEditorLoader`. Every other
 * test runs with the rejecting importer of `test/setup.ts`, so each one keeps the plain field.
 */
import type { KeyboardLayoutLike } from '@/lib/math-editor-loader';

/** The statics that the loader sets on the first resolve. */
export const MathfieldElement = {
  fontsDirectory: null as string | null,
  soundsDirectory: '/sounds' as string | null,
  keypressVibration: true,
};

class MathFieldDouble extends HTMLElement {
  latex = '';
  readOnly = false;
  /** The commands that ran, in order. */
  readonly commands: string[] = [];

  getValue(): string { return this.latex; }
  setValue(latex: string): void { this.latex = latex; }
  insert(latex: string): boolean {
    this.latex += latex;
    this.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  }
  executeCommand(name: string): boolean { this.commands.push(name); return true; }
  /** Test affordance: the learner typed, so the content changed and the field fired `input`. */
  type(latex: string): void {
    this.latex = latex;
    this.dispatchEvent(new Event('input', { bubbles: true }));
  }
}

if (!customElements.get('math-field')) customElements.define('math-field', MathFieldDouble);

/** What the keypad object records. `window.mathVirtualKeyboard` is this object while a test runs. */
export const mathVirtualKeyboard = {
  layouts: [] as ReadonlyArray<string | KeyboardLayoutLike>,
  visible: false,
  shows: 0,
  hides: 0,
  show() { this.visible = true; this.shows += 1; },
  hide() { this.visible = false; this.hides += 1; },
};

/** Install the keypad on `window`, as the real module does at import time. */
export function installKeyboard(): void {
  mathVirtualKeyboard.layouts = [];
  mathVirtualKeyboard.visible = false;
  mathVirtualKeyboard.shows = 0;
  mathVirtualKeyboard.hides = 0;
  Object.assign(window, { mathVirtualKeyboard });
}
