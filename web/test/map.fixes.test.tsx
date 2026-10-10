/**
 * The curriculum map, UX fixes: the opening frame, the find field, the remembered view,
 * the neighbor buttons, the back label, the heading and the zoom buttons.
 */
import { describe, expect, it } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { STATES } from '@/views/map/layout';
import { resetCytoscapeLoader } from '@/views/map/cytoscape-loader';
import { canvas, last, listButton, mount, mountLoaded, okImport } from './helpers/map';

describe('the map view — the fixes', () => {
  it('opens on the learner\'s own unit, and Show all fits the whole map', async () => {
    const view = await mountLoaded();
    const cy = last();
    expect(cy.selected).toEqual(['.st-frontier, .st-learning']);
    expect(cy.focuses).toBe(1);
    expect(cy.fits).toBe(0);
    await act(async () => { screen.getByRole('button', { name: 'Show all' }).click(); });
    expect(cy.fits).toBe(1);
    view.unmount();
  });

  it('zooms with the plus and minus buttons', async () => {
    const view = await mountLoaded();
    const before = last().zoom();
    await act(async () => { screen.getByRole('button', { name: 'Zoom in' }).click(); });
    expect(last().zoom()).toBeCloseTo(before * 1.25);
    await act(async () => { screen.getByRole('button', { name: 'Zoom out' }).click(); });
    expect(last().zoom()).toBeCloseTo(before);
    view.unmount();
  });

  it('shows one h1 reading Map, and a back label the parent chooses', async () => {
    const view = await mountLoaded({ backLabel: 'Back to lesson' });
    const heads = document.querySelectorAll('h1');
    expect(heads).toHaveLength(1);
    expect(heads[0].textContent).toBe('Map');
    expect(heads[0].className).toBe('h-screen');
    const back = screen.getByRole('button', { name: 'Back to lesson' });
    expect(back.compareDocumentPosition(heads[0]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    view.unmount();
  });

  it('keeps the list or map choice between visits', async () => {
    const view = await mountLoaded();
    await act(async () => { listButton().click(); });
    expect(window.localStorage.getItem('cadus.map.list')).toBe('1');
    await act(async () => { listButton().click(); });
    expect(window.localStorage.getItem('cadus.map.list')).toBe('0');
    view.unmount();
  });

  it('starts in the list when the stored choice says list', async () => {
    resetCytoscapeLoader(okImport);
    window.localStorage.setItem('cadus.map.list', '1');
    const view = await mount({}, undefined, true);
    expect(document.querySelector('.map-list')).not.toBeNull();
    expect(canvas().hidden).toBe(true);
    view.unmount();
  });

  it('filters the list by the find field', async () => {
    const view = await mountLoaded();
    await act(async () => { listButton().click(); });
    fireEvent.change(screen.getByLabelText('Find a topic'), { target: { value: 'dec' } });
    const rows = document.querySelectorAll('.map-list-group li');
    expect(rows).toHaveLength(1);
    expect(rows[0].textContent).toContain('Decimals');
    fireEvent.change(screen.getByLabelText('Find a topic'), { target: { value: 'zzz' } });
    expect(document.querySelectorAll('.map-list-group li')).toHaveLength(0);
    expect(screen.getByText('No topic matches "zzz".')).toBeTruthy();
    view.unmount();
  });

  it('centers the first match on the canvas', async () => {
    const view = await mountLoaded();
    fireEvent.change(screen.getByLabelText('Find a topic'), { target: { value: 'ratio' } });
    expect(last().centered.at(-1)).toBe('ratios');
    view.unmount();
  });

  it('lists the neighbors of a topic as buttons that select them', async () => {
    const view = await mountLoaded();
    const cy = last();
    await act(async () => { cy.emit('tap', cy.getElementById('fractions')); });
    const panel = document.querySelector<HTMLElement>('.map-panel')!;
    expect(panel.querySelector('.map-neighbors h3')!.textContent).toBe('Learn first');
    await act(async () => { screen.getByRole('button', { name: 'Ratios' }).click(); });
    expect(document.querySelector('.map-panel h2')!.textContent).toBe('Ratios');
    expect(cy.centered.at(-1)).toBe('ratios');
    view.unmount();
  });

  it('gives each state its own shape and the new words', () => {
    expect(new Set(STATES.map((s) => s.shape)).size).toBe(4);
    expect(STATES.map((s) => s.label)).toEqual([
      'Ready to learn', 'Learning', 'Shown by the starting questions',
      'Known before you started', 'Not reached',
    ]);
  });
});
