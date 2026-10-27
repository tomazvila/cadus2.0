/** The review pane lists the optional parts of a teach body above the raw body (D-TM1, D-TD1). */
import { describe, expect, it } from 'vitest';
import { screen } from '@testing-library/react';
import { docOf, mountReview, stubApi } from './helpers/admin';

describe('a teach document with optional parts', () => {
  it('lists the terms and the depth parts above the body', async () => {
    await mountReview(stubApi({
      getContent: async (digest) => ({
        ...docOf(digest),
        kind: 'teach',
        body: {
          concept: 'A rule.',
          worked_example: { problem: 'P', steps: ['S'] },
          terms: [{ term: 'Base', meaning: 'The number that is multiplied.' }],
          picture: 'Tiles in rows.',
          watch_out: 'It fails for sums.',
          second_example: { problem: 'A room.', steps: ['Read the area.'] },
        },
      }),
    }));
    expect(screen.getByText('Words of the page (1)')).toBeTruthy();
    expect(screen.getByText('Picture of the idea')).toBeTruthy();
    expect(screen.getByText('Where the method fails')).toBeTruthy();
    expect(screen.getByText('Second example')).toBeTruthy();
    expect(screen.getByText('The authored body')).toBeTruthy();
  });

  it('shows none of the sections for a body without them', async () => {
    await mountReview();
    expect(screen.queryByText('Picture of the idea')).toBeNull();
    expect(screen.queryByText(/Words of the page/)).toBeNull();
  });
});
