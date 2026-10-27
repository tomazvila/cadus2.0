/**
 * A figure with a slider (decision row D-FA1): the tween, the readouts, the keyboard and the
 * fail-closed reader. jsdom has no `Element.animate`, so each test installs a spy that
 * records its arguments and answers with a small fake `Animation`.
 */
import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import userEvent from '@testing-library/user-event';
import { axe } from 'vitest-axe';
import { MathVisual } from '@/components/MathVisual';
import { DEMO_SHADED_FIGURE, DEMO_SLOPE_FIGURE } from '@/api/demo-motion';
import { EASE, durationMs, keyframes, lawAt, readMotion } from '@/lib/motion';
import type { JsonValue } from '@/api/types';
import { sanitizeVisualSvg, type RenderedVisual } from '@/lib/visual';
import { AXE_IN_JSDOM } from './axe';
import { mountRoot, type Mounted } from './helpers/react';
import { setMedia } from './setup';

interface Call {
  element: Element;
  frames: Keyframe[];
  options: KeyframeAnimationOptions;
  animation: FakeAnimation;
}

interface FakeAnimation {
  effect: { getComputedTiming: () => { progress: number } };
  playState: string;
  cancel: () => void;
  onfinish: (() => void) | null;
  oncancel: (() => void) | null;
}

const REDUCED = '(prefers-reduced-motion: reduce)';
const roots: Array<() => void> = [];
let calls: Call[] = [];
let progress = 0;

beforeEach(() => {
  calls = [];
  progress = 0;
  Object.defineProperty(Element.prototype, 'animate', {
    configurable: true,
    writable: true,
    value(this: Element, frames: Keyframe[], options: KeyframeAnimationOptions) {
      const animation: FakeAnimation = {
        effect: { getComputedTiming: () => ({ progress }) },
        playState: 'running',
        cancel: vi.fn(),
        onfinish: null,
        oncancel: null,
      };
      calls.push({ element: this, frames, options, animation });
      return animation;
    },
  });
});

afterEach(() => {
  roots.splice(0).forEach((fn) => { fn(); });
  Reflect.deleteProperty(Element.prototype, 'animate');
  setMedia(REDUCED, false);
  vi.restoreAllMocks();
  vi.useRealTimers();
});

const mount = (visual: RenderedVisual): Mounted => mountRoot(<MathVisual visual={visual} />, roots);

/** Set the slider and fire the events of a drag step (`input` only) or a key press (both). */
function slide(view: Mounted, value: number, release = false): void {
  const input = view.find<HTMLInputElement>('input[type=range]');
  act(() => {
    input.value = String(value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    if (release) input.dispatchEvent(new Event('change', { bubbles: true }));
  });
}

const group = (view: Mounted, id: string): SVGElement => view.container.querySelector<SVGElement>(`[id="${id}"]`)!;

describe('the slider figure', () => {
  it('draws the figure, the slider and the readouts at the default', () => {
    const view = mount(DEMO_SLOPE_FIGURE);
    const input = view.find<HTMLInputElement>('input[type=range]');
    expect(input.min).toBe('0');
    expect(input.max).toBe('4');
    expect(input.value).toBe('1');
    expect(view.find('label.param-name').textContent).toBe('slope');
    expect(view.find('label.param-name').getAttribute('for')).toBe(input.id);
    expect(input.getAttribute('aria-valuetext')).toBe('1/2');
    expect(view.find('.param-value-text').textContent).toBe('1/2');
    expect(view.find('.param-readout-text').textContent).toBe('y = 1/2 x');
    expect(group(view, 'teach-1-m0').style.transform).toBe('matrix(1,0,0,1,0,0)');
  });

  it('tweens every changed group with transform only, 250 to 400 ms, ease-out-quart', () => {
    const view = mount(DEMO_SLOPE_FIGURE);
    slide(view, 3);
    expect(calls).toHaveLength(2);
    for (const call of calls) {
      expect(call.options.easing).toBe(EASE);
      expect(call.options.duration).toBeGreaterThanOrEqual(250);
      expect(call.options.duration).toBeLessThanOrEqual(400);
      expect(call.frames).toHaveLength(9);
      for (const frame of call.frames) expect(Object.keys(frame)).toEqual(['transform']);
    }
    expect(group(view, 'teach-1-m0').style.transform).toBe(
      lawAt(readMotion(DEMO_SLOPE_FIGURE.motion!, group(view, 'teach-1-m0').ownerSVGElement!)!.groups[0].law, 3).transform,
    );
  });

  it('scales the length with the distance and fades a revealed part with opacity only', () => {
    expect(durationMs(0, 1, 60)).toBeLessThan(256);
    expect(durationMs(0, 60, 60)).toBe(400);
    expect(durationMs(60, 0, 60)).toBe(400);
    const view = mount(DEMO_SHADED_FIGURE);
    slide(view, 6);
    expect(calls).toHaveLength(2);
    for (const call of calls) {
      for (const frame of call.frames) expect(Object.keys(frame)).toEqual(['opacity']);
    }
  });

  it('starts the first frame in the input handler, with no timer in between', () => {
    vi.useFakeTimers();
    const view = mount(DEMO_SLOPE_FIGURE);
    const before = vi.getTimerCount();
    const input = view.find<HTMLInputElement>('input[type=range]');
    input.value = '4';
    const started = performance.now();
    let seen = -1;
    let timersAtAnimate = -1;
    Reflect.defineProperty(Element.prototype, 'animate', {
      configurable: true,
      writable: true,
      value(this: Element) {
        seen = performance.now() - started;
        timersAtAnimate = vi.getTimerCount();
        return { effect: null, playState: 'running', cancel: vi.fn(), onfinish: null, oncancel: null };
      },
    });
    act(() => { input.dispatchEvent(new Event('input', { bubbles: true })); });
    expect(seen).toBeGreaterThanOrEqual(0);
    expect(seen).toBeLessThan(140);
    expect(timersAtAnimate).toBe(before);
  });

  it('retargets from the place the figure shows now', () => {
    const view = mount(DEMO_SLOPE_FIGURE);
    slide(view, 4);
    progress = 0.5;
    calls = [];
    slide(view, 0);
    const first = calls[0].frames[0].transform as string;
    const root = group(view, 'teach-1-m0').ownerSVGElement!;
    const law = readMotion(DEMO_SLOPE_FIGURE.motion!, root)!.groups[0].law;
    // The old tween ran from index 1 to 4 and sat at half way: index 2.5.
    expect(first).toBe(lawAt(law, 2.5).transform);
    expect(calls[0].frames).toEqual(keyframes(law, 2.5, 0));
  });

  it('jumps to the end state with reduced motion: no tween, no fade, no tint', () => {
    setMedia(REDUCED, true);
    const view = mount(DEMO_SLOPE_FIGURE);
    slide(view, 4, true);
    expect(calls).toHaveLength(0);
    expect(view.find('.param-value-text').textContent).toBe('2');
    expect(view.find('.param-readout-text').textContent).toBe('y = 2x');
    expect(group(view, 'teach-1-m0').style.transform).not.toBe('matrix(1,0,0,1,0,0)');
    expect(view.container.querySelector('.param-ghost')).toBeNull();
    setMedia(REDUCED, false);
    slide(view, 2);
    expect(calls.length).toBeGreaterThan(0);
  });

  it('fades the readouts and tints the value on release only', () => {
    const view = mount(DEMO_SLOPE_FIGURE);
    slide(view, 3);
    slide(view, 4);
    const quiet = calls.filter((call) => call.options.duration === 1800 || call.options.duration === 200);
    expect(quiet).toHaveLength(0);
    slide(view, 4, true);
    const tint = calls.filter((call) => call.element.classList.contains('param-flash'));
    expect(tint).toHaveLength(1);
    expect(tint[0].frames).toEqual([{ opacity: 1 }, { opacity: 0 }]);
    expect(tint[0].options.duration).toBeGreaterThanOrEqual(1500);
    expect(tint[0].options.duration).toBeLessThanOrEqual(2000);
    const ghost = view.container.querySelector('.param-ghost')!;
    expect(ghost.textContent).toBe('y = 1/2 x');
    const fades = calls.filter((call) => call.options.duration === 200);
    expect(fades).toHaveLength(2);
    expect(fades.map((call) => call.frames[0].opacity).sort()).toEqual([0, 1]);
    act(() => { calls.find((call) => call.element === ghost)!.animation.onfinish?.(); });
    expect(view.container.querySelector('.param-ghost')).toBeNull();
  });

  it('reads the end of a drag once, 250 ms after the last input', () => {
    vi.useFakeTimers();
    const view = mount(DEMO_SLOPE_FIGURE);
    const live = view.find('[aria-live=polite]');
    const start = live.textContent;
    const summary = readMotion(DEMO_SLOPE_FIGURE.motion!, group(view, 'teach-1-m0').ownerSVGElement!)!.summary;
    slide(view, 2);
    act(() => { vi.advanceTimersByTime(50); });
    slide(view, 3);
    act(() => { vi.advanceTimersByTime(50); });
    slide(view, 4);
    expect(live.textContent).toBe(start);
    act(() => { vi.advanceTimersByTime(249); });
    expect(live.textContent).toBe(start);
    act(() => { vi.advanceTimersByTime(1); });
    expect(live.textContent).toBe(summary[4]);
    expect(view.container.querySelector('desc')!.textContent).toBe(summary[4]);
  });

  it('names the value after the events that a key press makes', async () => {
    const view = mount(DEMO_SLOPE_FIGURE);
    const input = view.find<HTMLInputElement>('input[type=range]');
    const user = userEvent.setup();
    await user.click(input);
    input.focus();
    input.value = '1';
    await user.keyboard('{ArrowRight}');
    // user-event does not move a range input; the browser does. The phone check covers keys
    // in a real engine. Here the input event of that move proves the wiring.
    slide(view, 2, true);
    expect(input.getAttribute('aria-valuetext')).toBe('1');
    slide(view, 0, true);
    expect(input.getAttribute('aria-valuetext')).toBe('0');
    slide(view, 4, true);
    expect(input.getAttribute('aria-valuetext')).toBe('2');
  });

  it('has no axe violation by default and after one input', async () => {
    const view = mount(DEMO_SLOPE_FIGURE);
    expect(await axe(view.container, AXE_IN_JSDOM)).toHaveNoViolations();
    slide(view, 3, true);
    expect(await axe(view.container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('keeps ids apart when two figures share a page', async () => {
    const holder = document.createElement('div');
    document.body.append(holder);
    const first = mount(DEMO_SHADED_FIGURE);
    const second = mount(DEMO_SLOPE_FIGURE);
    expect(first.find('input').id).not.toBe(second.find('input').id);
    holder.append(...Array.from(first.container.childNodes).map((n) => n.cloneNode(true)),
      ...Array.from(second.container.childNodes).map((n) => n.cloneNode(true)));
    expect(await axe(holder, AXE_IN_JSDOM)).toHaveNoViolations();
    holder.remove();
  });
});

describe('the reader of the table', () => {
  const root = sanitizeVisualSvg(DEMO_SLOPE_FIGURE.svg)!;
  type Draft = {
    parameter: { n: number; labels: string[] };
    groups: Array<{ id: string; law: { m: number[][] } }>;
    summary: string[];
  };
  const good = DEMO_SLOPE_FIGURE.motion!;
  const read = (edit: (copy: Draft) => void) => {
    const copy = JSON.parse(JSON.stringify(good)) as Draft;
    edit(copy);
    return readMotion(copy, root);
  };

  it('accepts the table the server wrote', () => {
    expect(readMotion(good, root)).not.toBeNull();
  });

  it('refuses every table that does not fit the figure', () => {
    expect(read((m) => { m.groups[0].id = 'missing-1'; })).toBeNull();
    expect(read((m) => { m.groups[1].id = m.groups[0].id; })).toBeNull();
    expect(read((m) => { m.groups[0].id = 'a b'; })).toBeNull();
    expect(read((m) => { m.groups[0].id = 'a"b'; })).toBeNull();
    expect(read((m) => { m.parameter.n = 61; })).toBeNull();
    expect(read((m) => { m.parameter.labels.pop(); })).toBeNull();
    expect(read((m) => { m.groups[0].law.m[5][0] = Number.NaN; })).toBeNull();
    expect(read((m) => { m.groups = Array.from({ length: 13 }, () => m.groups[0]); })).toBeNull();
    expect(read((m) => { m.summary.pop(); })).toBeNull();
  });

  it('draws a still figure and no slider when the table is wrong', () => {
    const bad: RenderedVisual = { ...DEMO_SLOPE_FIGURE, motion: { ...(good as { [key: string]: JsonValue }), groups: [] } };
    const view = mount(bad);
    expect(view.container.querySelector('svg')).not.toBeNull();
    expect(view.container.querySelector('input[type=range]')).toBeNull();
    expect(view.container.textContent).toContain('A coordinate plane');
  });
});
