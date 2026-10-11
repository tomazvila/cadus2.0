import { describe, expect, it } from 'vitest';
import { frontierWaitText, reopensText } from '@/lib/reopen';

const NOW = new Date(2026, 9, 11, 9, 30);
const time = (d: Date): string =>
  d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });

describe('reopensText', () => {
  it('says today for the same local day', () => {
    const at = new Date(2026, 9, 11, 18, 4);
    expect(reopensText(at.toISOString(), NOW)).toBe(`today at ${time(at)}`);
  });

  it('says tomorrow for the next local day', () => {
    const at = new Date(2026, 9, 12, 0, 5);
    expect(reopensText(at.toISOString(), NOW)).toBe(`tomorrow at ${time(at)}`);
  });

  it('gives the date for a later day', () => {
    const at = new Date(2026, 9, 14, 18, 4);
    const date = at.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
    expect(reopensText(at.toISOString(), NOW)).toBe(`on ${date} at ${time(at)}`);
  });

  it('is empty for a value that does not parse', () => {
    expect(reopensText('not a time', NOW)).toBe('');
  });
});

describe('frontierWaitText', () => {
  const at = new Date(2026, 9, 12, 18, 4);
  it('names the lesson and the time', () => {
    const wait = { topic: { id: 'frac', name: 'Fractions' }, until: at.toISOString() };
    expect(frontierWaitText(wait, NOW)).toBe(
      `Fractions opens again tomorrow at ${time(at)}. A lesson with too many misses waits one day.`,
    );
  });

  it('falls back to the topic id', () => {
    const wait = { topic: { id: 'frac', name: null }, until: at.toISOString() };
    expect(frontierWaitText(wait, NOW)).toContain('frac opens again tomorrow');
  });
});
