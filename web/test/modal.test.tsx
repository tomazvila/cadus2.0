/**
 * The modal and the dialog context (S4).
 *
 * EVERY TEST HERE AWAITS THE PROMISE. That is the whole design of this file. A test that
 * asserts the overlay left the DOM proves nothing: React removes the portal for free on
 * unmount, so such a test passes with the settle machinery deleted — and the bug it was
 * supposed to catch is a call site that waits forever on a promise nobody resolves.
 *
 * The four ways out are cancel, Esc, the backdrop, and a superseding `open()`. The fifth is
 * the provider unmount, which is the one a view teardown takes.
 */
import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { DialogProvider, useDialogs, type Dialogs } from '@/components/Modal';
import { AXE_IN_JSDOM } from './axe';
import { allowConsoleError } from './setup';

/** A dialog with two controls, so the trap has a first and a last to wrap between. */
function confirmDialog(resolve: (v: string | null) => void) {
  return (
    <div className="modal" role="dialog" aria-modal="true" aria-label="Confirm">
      <h2>Confirm</h2>
      <button type="button" onClick={() => { resolve('yes'); }}>Confirm</button>
      <button type="button" onClick={() => { resolve(null); }}>Cancel</button>
    </div>
  );
}

/**
 * Mount a provider and hand the test the `open()` it exposes.
 *
 * The opener button is real, and it is focused before every `open()`: "focus returns to the
 * opener" is only an assertion when something held focus first.
 */
function mountDialogs() {
  const seen: Array<string | null> = [];
  let dialogs!: Dialogs;

  function Grab() {
    dialogs = useDialogs();
    return null;
  }

  // Into `#view`, the <main> index.html ships — not into a bare div. Landmark structure is
  // part of what the axe assertion below is checking.
  const view = render(
    <DialogProvider>
      <Grab />
      <button type="button">Open</button>
    </DialogProvider>,
    { container: document.getElementById('view')! },
  );

  const opener = screen.getByRole('button', { name: 'Open' });
  opener.focus();

  return {
    opener,
    seen,
    open: (render_: (r: (v: string | null) => void) => React.ReactNode = confirmDialog) => {
      let promise!: Promise<string | null>;
      act(() => { promise = dialogs.open<string>(render_); });
      void promise.then((v) => { seen.push(v); });
      return promise;
    },
    unmount: () => { act(() => { view.unmount(); }); },
  };
}

describe('Modal', () => {
  it('portals the dialog to document.body, outside the React container', () => {
    const d = mountDialogs();
    const promise = d.open();
    // `.modal-overlay` is `position: fixed; inset: 0`. Rendered inside a view that has a
    // transform or a stacking context of its own, it would be clipped to that box.
    expect(document.body.querySelector('.modal-overlay')).not.toBeNull();
    expect(screen.getByRole('dialog')).toBeTruthy();
    d.unmount();
    return expect(promise).resolves.toBeNull();
  });

  it('moves focus to the first control in the dialog', () => {
    const d = mountDialogs();
    const promise = d.open();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Confirm' }));
    d.unmount();
    return expect(promise).resolves.toBeNull();
  });

  it('closes on Esc and returns focus to the opener', async () => {
    const d = mountDialogs();
    const promise = d.open();
    expect(document.activeElement).not.toBe(d.opener);

    await act(async () => { fireEvent.keyDown(document, { key: 'Escape' }); });

    await expect(promise).resolves.toBeNull();
    expect(document.body.querySelector('.modal-overlay')).toBeNull();
    // Focus belongs back on the control that opened the dialog. Left on `<body>`, the next
    // Tab restarts at the top of the page.
    expect(document.activeElement).toBe(d.opener);
  });

  it('resolves the chosen value when a control settles it', async () => {
    const d = mountDialogs();
    const promise = d.open();
    await act(async () => { screen.getByRole('button', { name: 'Confirm' }).click(); });
    await expect(promise).resolves.toBe('yes');
  });

  it('resolves null on the backdrop, and not on a click inside the dialog', async () => {
    const d = mountDialogs();
    const promise = d.open();

    // A click that starts inside must not dismiss: the handler compares target to
    // currentTarget for exactly this.
    await act(async () => { fireEvent.click(screen.getByRole('dialog')); });
    expect(document.body.querySelector('.modal-overlay')).not.toBeNull();

    await act(async () => { fireEvent.click(document.body.querySelector('.modal-overlay')!); });
    await expect(promise).resolves.toBeNull();
  });

  it('resolves null when the provider unmounts under it', async () => {
    const d = mountDialogs();
    const promise = d.open();
    d.unmount();
    // The view that opened it has gone. Its `await` must not hang for the life of the tab.
    await expect(promise).resolves.toBeNull();
  });

  it('settles the incumbent before a second open supersedes it', async () => {
    const d = mountDialogs();
    const first = d.open();
    const second = d.open();

    // Reachable by a double click on one button. Before this, the first promise was
    // overwritten and its handler waited forever.
    await expect(first).resolves.toBeNull();
    expect(screen.getAllByRole('dialog')).toHaveLength(1);

    await act(async () => { screen.getByRole('button', { name: 'Confirm' }).click(); });
    await expect(second).resolves.toBe('yes');
    expect(d.seen).toEqual([null, 'yes']);
  });

  it('settles a promise once, even when Cancel and Esc both fire', async () => {
    const d = mountDialogs();
    const promise = d.open();
    await act(async () => {
      screen.getByRole('button', { name: 'Cancel' }).click();
      fireEvent.keyDown(document, { key: 'Escape' });
    });
    await expect(promise).resolves.toBeNull();
    expect(d.seen).toEqual([null]);
  });

  it('traps Tab at both ends of the dialog', () => {
    const d = mountDialogs();
    const promise = d.open();
    const first = screen.getByRole('button', { name: 'Confirm' });
    const last = screen.getByRole('button', { name: 'Cancel' });

    // `aria-modal="true"` tells a screen reader the page behind is inert. A Tab that walks
    // out to the opener makes that a lie.
    last.focus();
    fireEvent.keyDown(document, { key: 'Tab' });
    expect(document.activeElement).toBe(first);

    first.focus();
    fireEvent.keyDown(document, { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(last);

    d.unmount();
    return expect(promise).resolves.toBeNull();
  });

  it('pulls focus back in when it is already outside the dialog', () => {
    const d = mountDialogs();
    const promise = d.open();
    d.opener.focus();
    fireEvent.keyDown(document, { key: 'Tab' });
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Confirm' }));
    d.unmount();
    return expect(promise).resolves.toBeNull();
  });

  it('releases its key listener on unmount', async () => {
    const d = mountDialogs();
    const promise = d.open();
    d.unmount();
    await expect(promise).resolves.toBeNull();
    // A listener left on `document` reaches a resolver whose dialog is gone. Nothing on
    // screen changes and nothing throws — which is why this needs an explicit test.
    expect(() => { fireEvent.keyDown(document, { key: 'Escape' }); }).not.toThrow();
    expect(d.seen).toEqual([null]);
  });

  it('reports zero axe violations while open', async () => {
    const d = mountDialogs();
    const promise = d.open();
    const overlay = document.body.querySelector('.modal-overlay')!;
    expect(await axe(overlay, AXE_IN_JSDOM)).toHaveNoViolations();
    d.unmount();
    await expect(promise).resolves.toBeNull();
  });
});

describe('useDialogs', () => {
  it('refuses to run outside a provider instead of returning a dead open()', () => {
    // A silent no-op here is a promise that never settles at every call site.
    function Orphan() { useDialogs(); return null; }
    allowConsoleError(/useDialogs\(\) outside a DialogProvider/);
    allowConsoleError(/The above error occurred/);
    allowConsoleError(/An error occurred in/);
    expect(() => render(<Orphan />)).toThrow('useDialogs() outside a DialogProvider');
  });
});
