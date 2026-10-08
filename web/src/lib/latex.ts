/**
 * Repair LaTeX commands that a JSON decoder turned into control characters.
 *
 * A model that writes `\frac` inside a JSON string, with a single backslash, makes the
 * decoder read `\f` as a form feed. The text then holds U+000C followed by `rac{3}{4}`, and
 * KaTeX draws a box. The same happens to `\times` (tab), `\neq` (line feed), `\beta`
 * (backspace) and `\right` (carriage return). Inside a math segment a control character
 * followed by letters is never valid, so the command is put back.
 *
 * A repair needs the whole command name: the letters after the control character must spell
 * a known command and must not run on into more letters. `\n` + `u` + `m` stays as it is.
 */

/** The command endings that follow each control character, longest first. */
const ENDINGS: Readonly<Record<string, readonly string[]>> = {
  '\f': ['rac', 'orall'],
  '\t': ['imes', 'heta', 'ext', 'frac', 'riangle', 'an', 'au', 'o'],
  '\n': ['otin', 'abla', 'eq', 'ot', 'eg', 'u', 'e', 'i'],
  '\b': ['inom', 'eta', 'ar', 'igcup', 'igcap', 'egin'],
  '\r': ['ightarrow', 'ight', 'angle', 'floor', 'ceil', 'ho'],
};

/** One repair: the command that was restored. */
export interface LatexRepair {
  command: string;
}

const isLetter = (ch: string | undefined): boolean => ch !== undefined && /[A-Za-z]/.test(ch);

const COMMAND_START: Readonly<Record<string, string>> = { '\f': 'f', '\t': 't', '\n': 'n', '\b': 'b', '\r': 'r' };

/**
 * The text with every damaged command inside `$…$` and `$$…$$` restored.
 *
 * `onRepair` is called once per repair, so the caller can log it.
 */
export function repairLatex(text: string, onRepair?: (repair: LatexRepair) => void): string {
  if (!/[\f\t\n\b\r]/.test(text)) return text;
  let out = '';
  let inMath = false;
  let i = 0;
  while (i < text.length) {
    const ch = text[i]!;
    if (ch === '$') {
      inMath = !inMath;
      out += ch;
      i += 1;
      continue;
    }
    const endings = inMath ? ENDINGS[ch] : undefined;
    if (endings) {
      const rest = text.slice(i + 1);
      const hit = endings.find((end) => rest.startsWith(end) && !isLetter(rest[end.length]));
      if (hit !== undefined) {
        const command = `\\${COMMAND_START[ch]!}${hit}`;
        onRepair?.({ command });
        out += command;
        i += 1 + hit.length;
        continue;
      }
    }
    out += ch;
    i += 1;
  }
  return out;
}
