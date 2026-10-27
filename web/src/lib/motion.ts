/**
 * The pure half of a figure with a slider (decision row D-FA1).
 *
 * The server draws ONE SVG at the default value. It sends a table beside the SVG: a law per
 * moving group in the slider index, the exact text of each step, and the exact text
 * equivalent of each step. This file reads that table and turns a law and an index into the
 * CSS the group needs. It does no geometry: it evaluates `k * i + c` for numbers the server
 * wrote, and it prints text the server wrote.
 *
 * Only `transform` and `opacity` ever change. Both run on the compositor, so a tween holds
 * its frame rate while a script blocks the main thread. The Web Animations API and the CSSOM
 * write them after the sanitizer has rebuilt the figure, so the sanitizer allowlist stays as
 * it was: no `transform`, `style` or `opacity` attribute enters from the server, and a server
 * SVG that carries one still fails closed. The CSP allows this: it blocks inline script and
 * `eval`, and neither a CSSOM write nor `Element.animate` is script.
 */
import type { JsonValue } from '@/api/types';

/**
 * The server rounds each coefficient to 4 decimals. At the largest step count (60) and the
 * largest offset in a 480 unit view box, the error is under 0.01 px, so no step shows it.
 */

/** The ease-out-quart curve of every tween. */
export const EASE = 'cubic-bezier(0.25, 1, 0.5, 1)';

/** The most steps one slider has. The server refuses more. */
const MAX_STEPS = 60;

/** The most moving groups one figure has. The server refuses more. */
const MAX_GROUPS = 12;

/** The count of sampled keyframes of one tween. */
const SAMPLES = 9;

/** One law: how a group moves with the slider index. */
export type Law =
  | { kind: 'matrix'; m: Array<[number, number]> }
  | { kind: 'rotate'; deg: [number, number]; cx: number; cy: number }
  | { kind: 'reveal'; from: number };

/** The slider. */
interface ParameterTable {
  name: string;
  unit: string;
  labels: string[];
  default_index: number;
  n: number;
}

/** One moving group. */
export interface GroupLaw {
  id: string;
  law: Law;
}

/** One readout line under the slider. */
interface Readout {
  label: string;
  texts: string[];
}

/** The table the server sends beside a figure that moves. */
export interface Motion {
  parameter: ParameterTable;
  groups: GroupLaw[];
  readouts: Readout[];
  summary: string[];
}

/** The CSS that a law writes at one index. */
export interface GroupStyle {
  transform?: string;
  opacity?: string;
}

const ID = /^[A-Za-z0-9_-]{1,64}$/;

type Raw = JsonValue | undefined;
type RawObject = { [key: string]: JsonValue };

const isObject = (value: Raw): value is RawObject =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const finite = (value: Raw): value is number => typeof value === 'number' && Number.isFinite(value);

const whole = (value: Raw, low: number, high: number): value is number =>
  typeof value === 'number' && Number.isInteger(value) && value >= low && value <= high;

const pair = (value: Raw): value is [number, number] =>
  Array.isArray(value) && value.length === 2 && finite(value[0]) && finite(value[1]);

const strings = (value: Raw, length: number): value is string[] =>
  Array.isArray(value) && value.length === length && value.every((item) => typeof item === 'string');

function readLaw(raw: Raw, n: number): Law | null {
  if (!isObject(raw)) return null;
  if (raw.kind === 'matrix') {
    const m = raw.m;
    if (!Array.isArray(m) || m.length !== 6 || !m.every(pair)) return null;
    return { kind: 'matrix', m: m.map((entry) => [entry[0], entry[1]] as [number, number]) };
  }
  if (raw.kind === 'rotate') {
    if (!pair(raw.deg) || !finite(raw.cx) || !finite(raw.cy)) return null;
    return { kind: 'rotate', deg: [raw.deg[0], raw.deg[1]], cx: raw.cx, cy: raw.cy };
  }
  if (raw.kind === 'reveal' && whole(raw.from, 0, n + 1)) return { kind: 'reveal', from: raw.from };
  return null;
}

/** How many elements of `root` carry this id. */
function countId(root: Element, id: string): number {
  let count = root.getAttribute('id') === id ? 1 : 0;
  for (const node of Array.from(root.querySelectorAll('[id]'))) {
    if (node.getAttribute('id') === id) count += 1;
  }
  return count;
}

function readParameter(raw: Raw): ParameterTable | null {
  if (!isObject(raw)) return null;
  const { n, default_index: defaultIndex, name, unit, labels } = raw;
  if (!whole(n, 1, MAX_STEPS) || !whole(defaultIndex, 0, n)) return null;
  if (typeof name !== 'string' || name.trim() === '' || typeof unit !== 'string') return null;
  if (!strings(labels, n + 1)) return null;
  return { name, unit, labels, default_index: defaultIndex, n };
}

function readGroups(raw: Raw, n: number, root: Element): GroupLaw[] | null {
  if (!Array.isArray(raw) || raw.length < 1 || raw.length > MAX_GROUPS) return null;
  const groups: GroupLaw[] = [];
  for (const group of raw) {
    if (!isObject(group) || typeof group.id !== 'string' || !ID.test(group.id)) return null;
    const law = readLaw(group.law, n);
    if (law === null || countId(root, group.id) !== 1) return null;
    if (groups.some((seen) => seen.id === group.id)) return null;
    groups.push({ id: group.id, law });
  }
  return groups;
}

function readReadouts(raw: Raw, n: number): Readout[] | null {
  if (!Array.isArray(raw) || raw.length > 8) return null;
  const lines: Readout[] = [];
  for (const line of raw) {
    if (!isObject(line) || typeof line.label !== 'string' || !strings(line.texts, n + 1)) return null;
    lines.push({ label: line.label, texts: line.texts });
  }
  return lines;
}

/**
 * Read the table the server sent, or `null` when anything is wrong.
 *
 * One failure gives `null`, and the figure then draws still with no slider. The reader never
 * builds half a slider.
 */
export function readMotion(raw: Raw, root: Element): Motion | null {
  if (!isObject(raw)) return null;
  const parameter = readParameter(raw.parameter);
  if (parameter === null) return null;
  const groups = readGroups(raw.groups, parameter.n, root);
  const readouts = readReadouts(raw.readouts, parameter.n);
  const summary = raw.summary;
  if (groups === null || readouts === null || !strings(summary, parameter.n + 1)) return null;
  return { parameter, groups, readouts, summary };
}

/** One number as CSS text, rounded to four places. */
function css(value: number): string {
  const rounded = Math.round(value * 10_000) / 10_000;
  return String(rounded === 0 ? 0 : rounded);
}

/** The CSS a law writes at slider index `index`. The index may be fractional. */
export function lawAt(law: Law, index: number): GroupStyle {
  if (law.kind === 'matrix') {
    const entries = law.m.map(([k, c]) => css(k * index + c));
    return { transform: `matrix(${entries.join(',')})` };
  }
  if (law.kind === 'rotate') {
    const degrees = css(law.deg[0] * index + law.deg[1]);
    const cx = `${css(law.cx)}px`;
    const cy = `${css(law.cy)}px`;
    return { transform: `translate(${cx},${cy}) rotate(${degrees}deg) translate(${css(-law.cx)}px,${css(-law.cy)}px)` };
  }
  // A part fades in while the slider passes it: 0 below `from - 1`, 1 from `from`.
  const shown = Math.min(1, Math.max(0, index - law.from + 1));
  return { opacity: css(shown) };
}

/** The property a law animates. */
export function propertyOf(law: Law): 'transform' | 'opacity' {
  return law.kind === 'reveal' ? 'opacity' : 'transform';
}

/**
 * Sampled keyframes from index `from` to index `to`.
 *
 * The browser interpolates two `matrix()` values by decomposition, and a shear then passes
 * through wrong lines. Nine samples at equal steps of the index keep each frame between two
 * close, true states.
 */
export function keyframes(law: Law, from: number, to: number, count = SAMPLES): Keyframe[] {
  const frames: Keyframe[] = [];
  for (let step = 0; step < count; step += 1) {
    const at = from + ((to - from) * step) / (count - 1);
    const style = lawAt(law, at);
    frames.push(law.kind === 'reveal' ? { opacity: style.opacity! } : { transform: style.transform! });
  }
  return frames;
}

/** The length of a tween: 250 ms for a tiny move, 400 ms for the full range. */
export function durationMs(from: number, to: number, n: number): number {
  return Math.round(250 + 150 * Math.min(1, Math.abs(to - from) / n));
}
