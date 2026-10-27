/**
 * The slider under a figure that moves (decision row D-FA1).
 *
 * One native range input walks the index of the server table. The name of the parameter is
 * its label. The value sits in an output beside the readout lines the server wrote, and a
 * hidden live region reads the end of a drag once. Arrow keys, Home, End, PageUp and PageDown
 * work because the input is native.
 */
import { useId, useRef, type RefObject } from 'react';
import type { Motion } from '@/lib/motion';
import { useFigureMotion } from '@/components/useFigureMotion';

export interface ParameterSliderProps {
  /** The table the server sent. */
  motion: Motion;
  /** The frame that holds the rebuilt SVG. */
  frame: RefObject<HTMLElement | null>;
}

export function ParameterSlider({ motion, frame }: ParameterSliderProps) {
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  const flash = useRef<HTMLSpanElement>(null);
  const readouts = useRef<HTMLDivElement>(null);
  const { index, live } = useFigureMotion(motion, { frame, input, flash, readouts });
  const { parameter } = motion;
  const text = parameter.labels[index] ?? '';
  return (
    <div className="param">
      <label className="param-name" htmlFor={id}>{parameter.name}</label>
      <input
        id={id}
        ref={input}
        className="param-input"
        type="range"
        min={0}
        max={parameter.n}
        step={1}
        defaultValue={parameter.default_index}
        aria-valuetext={text}
      />
      <output className="param-value" htmlFor={id} aria-live="off">
        <span className="param-flash" ref={flash} aria-hidden="true" />
        <span className="param-value-text">{text}</span>
      </output>
      <div className="param-readouts" ref={readouts}>
        {motion.readouts.map((readout, line) => (
          <p className="param-readout" data-line={line} key={readout.label + String(line)}>
            <span className="param-readout-label">{readout.label}</span>{' '}
            <span className="param-readout-slot">
              <span className="param-readout-text">{readout.texts[index]}</span>
            </span>
          </p>
        ))}
      </div>
      <div className="visually-hidden" aria-live="polite">{live}</div>
    </div>
  );
}
