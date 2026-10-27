/**
 * The engine of a figure with a slider (decision row D-FA1). It holds no React state.
 *
 * The rules come from the owner's notes on motion:
 *
 *  - Only `transform` and `opacity` change. Both stay off the main thread.
 *  - The first changed frame starts in the `input` handler: no timer, no debounce. The
 *    learner sees the cause within 140 ms.
 *  - A tween lasts 250 to 400 ms with ease-out-quart. No transform tween is longer.
 *  - A new input during a tween starts from the place the figure shows now, so the figure
 *    never jumps back. Sampled keyframes keep each frame between two true figures.
 *  - On `change` (release, or one key press) the changed readouts crossfade (200 ms) and a
 *    tint fades out over 1.8 s, the yellow fade of the notes. A drag shows the figure move
 *    and the number follow, and nothing else, so the signal does not habituate.
 *  - With `prefers-reduced-motion: reduce`, or with no Web Animations API, every change
 *    jumps to its end state: no tween, no fade. The CSS rule of `app.css` does not stop the
 *    Web Animations API, so this branch is load-bearing.
 *  - `will-change` is set only while a tween runs.
 */
import { EASE, durationMs, keyframes, lawAt, propertyOf, type GroupLaw, type Motion } from '@/lib/motion';

/** The slow tint of a changed value, in ms (the yellow fade of the notes: 1.5 to 2 s). */
const FLASH_MS = 1800;

/** The crossfade of a changed readout, in ms. */
const FADE_MS = 200;

/** The wait before the live region reads the end of a drag, in ms. */
const LIVE_MS = 250;

const REDUCED = '(prefers-reduced-motion: reduce)';

/** Whether this browser may tween: it has the API and the learner did not ask for less. */
function mayAnimate(): boolean {
  if (typeof Element === 'undefined' || typeof Element.prototype.animate !== 'function') return false;
  if (typeof window.matchMedia !== 'function') return true;
  return !window.matchMedia(REDUCED).matches;
}

interface Running {
  animation: Animation;
  element: SVGElement;
}

/** What the engine tells its owner. */
export interface TweenSink {
  /** The slider index changed: print the value text now. */
  shown: (index: number) => void;
  /** The end of a drag settled: the live region reads this text. */
  announce: (text: string) => void;
}

/** The DOM the engine writes. */
export interface TweenDom {
  frame: HTMLElement;
  /** The tint element inside the value output. */
  flash: HTMLElement | null;
  /** The element that holds the readout lines. */
  readouts: HTMLElement | null;
}

function write(element: SVGElement, law: GroupLaw['law'], index: number): void {
  const style = lawAt(law, index);
  if (style.transform !== undefined) element.style.transform = style.transform;
  if (style.opacity !== undefined) element.style.opacity = style.opacity;
}

export class FigureTween {
  private from: number;
  private to: number;
  private running: Running[] = [];
  private committed: number;
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private readonly motion: Motion,
    private readonly dom: TweenDom,
    private readonly sink: TweenSink,
  ) {
    this.from = motion.parameter.default_index;
    this.to = this.from;
    this.committed = this.from;
  }

  private element(id: string): SVGElement | null {
    return this.dom.frame.querySelector<SVGElement>(`[id="${id}"]`);
  }

  /** Write the default state before the first paint. */
  start(): void {
    for (const group of this.motion.groups) {
      const element = this.element(group.id);
      if (element === null) continue;
      if (group.law.kind === 'matrix') element.classList.add('cadus-visual-shear');
      write(element, group.law, this.to);
    }
  }

  /** The index the figure shows now, read from the running tween. */
  current(): number {
    const lead = this.running[0]?.animation;
    if (lead === undefined || lead.playState === 'finished') return this.to;
    const progress = lead.effect?.getComputedTiming().progress ?? 0;
    return this.from + (this.to - this.from) * (progress ?? 0);
  }

  private cancel(): void {
    for (const { animation, element } of this.running) {
      animation.onfinish = null;
      animation.oncancel = null;
      animation.cancel();
      element.style.willChange = '';
    }
    this.running = [];
  }

  /** The slider moved to `next`: move the figure and print the value, in one call stack. */
  move(next: number): void {
    const from = this.current();
    this.cancel();
    this.from = from;
    this.to = next;
    const animate = mayAnimate();
    for (const group of this.motion.groups) {
      const element = this.element(group.id);
      if (element === null) continue;
      const before = lawAt(group.law, from);
      const after = lawAt(group.law, next);
      write(element, group.law, next);
      const changed = before.transform !== after.transform || before.opacity !== after.opacity;
      if (animate && changed) this.tween(element, group, from, next);
    }
    this.sink.shown(next);
    this.later(next);
  }

  private tween(element: SVGElement, group: GroupLaw, from: number, next: number): void {
    element.style.willChange = propertyOf(group.law);
    const animation = element.animate(keyframes(group.law, from, next), {
      duration: durationMs(from, next, this.motion.parameter.n),
      easing: EASE,
    });
    const settle = () => {
      element.style.willChange = '';
      this.running = this.running.filter((entry) => entry.animation !== animation);
    };
    animation.onfinish = settle;
    animation.oncancel = settle;
    this.running.push({ animation, element });
  }

  /** The live region, the SVG description and the printed text read the end of a drag once. */
  private later(next: number): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      const text = this.motion.summary[next] ?? '';
      this.sink.announce(text);
      const desc = this.dom.frame.querySelector('desc');
      if (desc !== null) desc.textContent = text;
      const caption = this.dom.frame.closest('figure')?.querySelector('.math-visual-summary');
      if (caption) caption.textContent = text;
    }, LIVE_MS);
  }

  /** The slider was released, or one key press ended: fade the changed values. */
  commit(next: number): void {
    const before = this.committed;
    this.committed = next;
    if (!mayAnimate()) return;
    this.dom.flash?.animate([{ opacity: 1 }, { opacity: 0 }], { duration: FLASH_MS, easing: 'ease-out' });
    const lines = this.dom.readouts;
    if (lines === null) return;
    this.motion.readouts.forEach((readout, line) => {
      if (readout.texts[before] === readout.texts[next]) return;
      const slot = lines.querySelector<HTMLElement>(`[data-line="${line}"] .param-readout-slot`);
      const real = slot?.querySelector<HTMLElement>('.param-readout-text');
      if (!slot || !real) return;
      const ghost = document.createElement('span');
      ghost.className = 'param-ghost';
      ghost.setAttribute('aria-hidden', 'true');
      ghost.textContent = readout.texts[before] ?? '';
      slot.appendChild(ghost);
      const fade = ghost.animate([{ opacity: 1 }, { opacity: 0 }], { duration: FADE_MS, easing: EASE });
      const remove = () => { ghost.remove(); };
      fade.onfinish = remove;
      fade.oncancel = remove;
      real.animate([{ opacity: 0 }, { opacity: 1 }], { duration: FADE_MS, easing: EASE });
    });
  }

  /** Stop every tween and every timer. */
  dispose(): void {
    this.cancel();
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}
