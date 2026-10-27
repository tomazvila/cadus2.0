/**
 * The hook that joins the range input of a figure to its tween engine (decision row D-FA1).
 *
 * The native `input` event starts the tween in the same call stack. React prints the value
 * text through `flushSync`, so the number changes in the handler too. `lib/figure-tween.ts`
 * holds the rules.
 */
import { useLayoutEffect, useState, type RefObject } from 'react';
import { flushSync } from 'react-dom';
import { FigureTween } from '@/lib/figure-tween';
import type { Motion } from '@/lib/motion';

export interface FigureMotionRefs {
  /** The frame that holds the rebuilt SVG. */
  frame: RefObject<HTMLElement | null>;
  /** The range input. */
  input: RefObject<HTMLInputElement | null>;
  /** The tint element inside the value output. */
  flash: RefObject<HTMLElement | null>;
  /** The element that holds the `.param-readout` lines. */
  readouts: RefObject<HTMLElement | null>;
}

export interface FigureMotionState {
  /** The index the readouts show. */
  index: number;
  /** The text the live region holds. */
  live: string;
}

export function useFigureMotion(motion: Motion, refs: FigureMotionRefs): FigureMotionState {
  const first = motion.parameter.default_index;
  const [index, setIndex] = useState(first);
  const [live, setLive] = useState(motion.summary[first] ?? '');

  // The default state is written before the first paint, so a reveal group and a nested
  // transform never flash through the wrong state. The listeners join in the same effect.
  useLayoutEffect(() => {
    const frame = refs.frame.current;
    const input = refs.input.current;
    if (frame === null || input === null) return;
    const engine = new FigureTween(
      motion,
      { frame, flash: refs.flash.current, readouts: refs.readouts.current },
      {
        shown: (next) => { flushSync(() => { setIndex(next); }); },
        announce: setLive,
      },
    );
    engine.start();
    const read = (): number | null => {
      const value = Number(input.value);
      return Number.isInteger(value) && value >= 0 && value <= motion.parameter.n ? value : null;
    };
    const onInput = () => {
      const next = read();
      if (next !== null) engine.move(next);
    };
    const onChange = () => {
      const next = read();
      if (next !== null) engine.commit(next);
    };
    input.addEventListener('input', onInput);
    input.addEventListener('change', onChange);
    return () => {
      input.removeEventListener('input', onInput);
      input.removeEventListener('change', onChange);
      engine.dispose();
    };
  }, [motion, refs.frame, refs.input, refs.flash, refs.readouts]);

  return { index, live };
}
