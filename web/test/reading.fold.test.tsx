/** The reading-list fold (D-RL1): label, closed start, links, chapter notes, tags. */
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ReadingFold } from '@/components/ReadingFold';
import { Teach } from '@/views/session/Teach';
import type { ReadingEntry } from '@/api/types';
import { LESSON, TEACHING } from './helpers/session';

const LINK: ReadingEntry = {
  title: 'Free Primer', author: 'A. Writer', access: 'free',
  where: 'https://example.org/primer', note: 'Short pages with examples.',
};
const CHAPTER: ReadingEntry = {
  title: 'Paper Book', author: 'B. Writer', access: 'book',
  where: 'Chapter 2, Fractions', note: 'Practice problems at the back.',
};

describe('ReadingFold', () => {
  it('starts closed under the label "Read more about this course"', () => {
    const { container } = render(<ReadingFold reading={[LINK, CHAPTER]} />);
    expect(screen.getByText('Read more about this course')).toBeTruthy();
    expect((container.querySelector('details') as HTMLDetailsElement).open).toBe(false);
  });

  it('opens an https address in a new tab with a safe rel', () => {
    render(<ReadingFold reading={[LINK, CHAPTER]} />);
    const link = screen.getByText('Open Free Primer in a new tab') as HTMLAnchorElement;
    expect(link.getAttribute('href')).toBe(LINK.where);
    expect(link.getAttribute('target')).toBe('_blank');
    expect(link.getAttribute('rel')).toBe('noopener noreferrer');
    expect(document.querySelectorAll('a').length).toBe(1);
  });

  it('shows a chapter note as plain text and both tags', () => {
    render(<ReadingFold reading={[LINK, CHAPTER]} />);
    expect(screen.getByText('Where to read: Chapter 2, Fractions')).toBeTruthy();
    expect(screen.getByText('Free to read')).toBeTruthy();
    expect(screen.getByText('Book')).toBeTruthy();
    expect(screen.getByText(/Free Primer, A. Writer/)).toBeTruthy();
  });

  it('never makes a link of a javascript address', () => {
    render(<ReadingFold reading={[{ ...LINK, where: 'javascript:alert(1)' }]} />);
    expect(document.querySelector('a')).toBeNull();
    expect(screen.getByText('Where to read: javascript:alert(1)')).toBeTruthy();
  });

  it('renders nothing for an empty or absent list', () => {
    expect(render(<ReadingFold reading={[]} />).container.innerHTML).toBe('');
    expect(render(<ReadingFold reading={undefined} />).container.innerHTML).toBe('');
  });
});

describe('the fold on the teach page', () => {
  it('sits under the worked example and before "Start practice"', () => {
    const { container } = render(
      <Teach task={LESSON} instruction={{ ...TEACHING, reading: [LINK] }} onContinue={() => {}} />,
    );
    const fold = container.querySelector('.reading-fold')!;
    const example = container.querySelector('.teach-example')!;
    const start = screen.getByRole('button', { name: /Start practice/ });
    expect(example.compareDocumentPosition(fold) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(fold.compareDocumentPosition(start) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
