/**
 * One gate run, rendered (A6).
 *
 * The three shapes `operator.rs` `gate_json` writes are three different facts, and each one
 * gets its own sentence. A sampled run reads as a warning, never as a tick.
 */
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { GateBlock, SAMPLED_LINE, shortDigest } from '@/views/admin/GateBlock';
import type { OperatorGateNote } from '@/api/types';

const note = (over: Partial<OperatorGateNote> = {}): OperatorGateNote => ({
  kp_id: 'algebra:linear',
  digest: 'abcdef0123456789',
  gated: true,
  notes: [],
  ...over,
});

const line = () => document.querySelector('.gate-line')!;

describe('GateBlock', () => {
  it('shortens the digest for the heading and keeps the whole one in the title', () => {
    render(<GateBlock note={note()} />);
    expect(shortDigest('abcdef0123456789')).toBe('abcdef012345');
    const head = document.querySelector('.gate-head .mono')!;
    expect(head.textContent).toBe('abcdef012345');
    expect(head.getAttribute('title')).toBe('abcdef0123456789');
  });

  it('says a knowledge point the curriculum does not name was not gated, with its reason', () => {
    render(<GateBlock note={note({ gated: false, reason: 'not in the curriculum' })} />);
    expect(line().className).toBe('gate-line gate-warn');
    expect(line().textContent).toBe('Not gated. not in the curriculum');
  });

  it('says not gated alone when no reason came', () => {
    render(<GateBlock note={note({ gated: false })} />);
    expect(line().textContent).toBe('Not gated. ');
  });

  it('names the code and the message of a refused body', () => {
    render(<GateBlock note={note({ rejected: { code: 'no_solution', message: 'No draw solves it.' } })} />);
    expect(line().className).toBe('gate-line gate-bad');
    expect(line().textContent).toBe('Rejected — no_solution: No draw solves it.');
  });

  it('reads an exhaustive pass as good, with its count', () => {
    render(<GateBlock note={note({ exhaustive: true, instances_checked: 4096 })} />);
    expect(line().className).toBe('gate-line gate-good');
    expect(line().textContent).toBe('Exhaustive: the gate walked every satisfying tuple. 4096 instances checked.');
  });

  it('reads a sampled pass as a warning, and an absent count as zero', () => {
    render(<GateBlock note={note({ exhaustive: false })} />);
    expect(line().className).toBe('gate-line gate-warn');
    expect(line().textContent).toBe(`${SAMPLED_LINE} 0 instances checked.`);
  });

  it('renders the notes verbatim, and no list when there is none', () => {
    const { unmount } = render(<GateBlock note={note({ notes: ['first', 'first'] })} />);
    expect(Array.from(document.querySelectorAll('.gate-notes li')).map((li) => li.textContent))
      .toEqual(['first', 'first']);
    unmount();
    render(<GateBlock note={note()} />);
    expect(document.querySelector('.gate-notes')).toBeNull();
  });
});
