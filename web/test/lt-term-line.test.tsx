/** The Lithuanian exam term line (D-VBE1): shown with a term, hidden without one. */
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LtTermLine } from '@/views/session/TeachExtras';

describe('LtTermLine', () => {
  it('shows the term in one plain line', () => {
    render(<LtTermLine term="Lygtys" />);
    expect(screen.getByText('Lithuanian exam term: Lygtys')).toBeTruthy();
  });

  it('renders nothing without a term', () => {
    const none = render(<LtTermLine term={undefined} />);
    expect(none.container.innerHTML).toBe('');
    const empty = render(<LtTermLine term="" />);
    expect(empty.container.innerHTML).toBe('');
  });
});
