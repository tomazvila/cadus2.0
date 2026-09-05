/**
 * The pure display helpers (S4), byte for byte the 1.0 originals except `fmtClock`.
 */
import { describe, expect, it } from 'vitest';
import { clamp01, fmtClock, num, pct, signed } from '@/lib/format';

describe('num', () => {
  it('gives a finite number back, and the fallback for everything else', () => {
    expect(num(3)).toBe(3);
    expect(num('0.0100')).toBe(0.01);
    expect(num(null)).toBe(0);
    expect(num(undefined)).toBe(0);
    expect(num(undefined, 40)).toBe(40);
    expect(num('nope', 5)).toBe(5);
    expect(num(Number.POSITIVE_INFINITY, 7)).toBe(7);
  });
});

describe('clamp01 and pct', () => {
  it('clamps to the unit interval and rounds the percent', () => {
    expect(clamp01(0.25)).toBe(0.25);
    expect(clamp01(-1)).toBe(0);
    expect(clamp01(2)).toBe(1);
    expect(clamp01('x')).toBe(0);
    expect(pct(0.185)).toBe(19);
    expect(pct(0.18)).toBe(18);
    expect(pct(null)).toBe(0);
  });
});

describe('signed', () => {
  it('prefixes a plus and never writes +-', () => {
    expect(signed(5)).toBe('+5');
    expect(signed(0)).toBe('+0');
    expect(signed(-4)).toBe('-4');
    expect(signed(undefined)).toBe('+0');
    expect(signed('x', 2)).toBe('+2');
  });
});

describe('fmtClock', () => {
  it('formats m:SS, floored, never negative, and paints 0:00 for a non-number', () => {
    expect(fmtClock(0)).toBe('0:00');
    expect(fmtClock(3)).toBe('0:03');
    expect(fmtClock(65.9)).toBe('1:05');
    expect(fmtClock(600)).toBe('10:00');
    expect(fmtClock(-5)).toBe('0:00');
    expect(fmtClock('nope')).toBe('0:00');
    expect(fmtClock(Number.POSITIVE_INFINITY)).toBe('0:00');
  });
});
