/**
 * The memoized MathLive import.
 *
 * It has the shape of `views/map/cytoscape-loader.ts`: a root-absolute runtime URL, a
 * constant specifier with `@vite-ignore`, and a memo that clears itself when the import
 * rejects. A rejected promise stays truthy. If the memo kept it, every later mount would
 * await the same dead promise, and only a page reload would recover.
 *
 * The npm package `mathlive` is NOT a dependency. The library sits in
 * `public/vendor/mathlive/` and `vite.config.ts` marks `/vendor/**` external.
 */

/** The members of the math field that the answer field calls. */
export interface MathFieldLike extends HTMLElement {
  /** The content in the named format. The answer uses `latex`. */
  getValue: (format?: string) => string;
  setValue: (latex: string) => void;
  insert: (latex: string) => boolean;
  focus: () => void;
  blur: () => void;
  readOnly: boolean;
  executeCommand: (name: string) => boolean;
}

/** The statics that the loader sets on the first resolve. */
export interface MathfieldStatics {
  fontsDirectory: string | null;
  soundsDirectory: string | null;
  keypressVibration: boolean;
}

/** One keycap, in the shape MathLive reads. */
export interface KeycapLike {
  label?: string;
  latex?: string;
  width?: 0.5 | 1.0 | 1.5 | 2.0 | 5.0;
  class?: string;
  tooltip?: string;
  variants?: string[];
}

/** One layout of the virtual keypad: a title and the rows of one layer. */
export interface KeyboardLayoutLike {
  label?: string;
  tooltip?: string;
  rows: KeycapLike[][];
}

/** The keypad object that the module puts on `window.mathVirtualKeyboard`. */
export interface VirtualKeyboardLike {
  layouts: ReadonlyArray<string | KeyboardLayoutLike>;
  show: (options?: { animate: boolean }) => void;
  hide: (options?: { animate: boolean }) => void;
}

export interface MathEditorModule {
  MathfieldElement: MathfieldStatics;
}

/** What the loader awaits. The test seam replaces it; production never does. */
export type MathEditorImporter = () => Promise<MathEditorModule>;

/** The runtime URL of the vendored library. Root-absolute: it is served, not bundled. */
export const MATHLIVE_URL = '/vendor/mathlive/mathlive.min.mjs';

/** The directory of the vendored fonts. The CSP allows fonts from the same origin only. */
export const MATHLIVE_FONTS_DIR = '/vendor/mathlive/fonts';

const vendorImport: MathEditorImporter = () =>
  import(/* @vite-ignore */ MATHLIVE_URL) as Promise<MathEditorModule>;

let importer: MathEditorImporter = vendorImport;
let pending: Promise<MathEditorModule> | null = null;

/** The library, loaded once per page and shared by every caller after that. */
export function loadMathEditor(): Promise<MathEditorModule> {
  if (!pending) {
    pending = importer()
      .then((module) => {
        module.MathfieldElement.fontsDirectory = MATHLIVE_FONTS_DIR;
        module.MathfieldElement.soundsDirectory = null;
        module.MathfieldElement.keypressVibration = false;
        return module;
      })
      .catch((err: Error) => {
        pending = null;
        throw err;
      });
  }
  return pending;
}

/** Test seam: drop the memo, and optionally swap the importer. No argument restores the vendored import. */
export function resetMathEditorLoader(next?: MathEditorImporter): void {
  pending = null;
  importer = next ?? vendorImport;
}
