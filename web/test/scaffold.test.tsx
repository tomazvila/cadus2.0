/**
 * Scaffolding smoke test (S1).
 *
 * Proves the toolchain is real before a single behavioral test is written: the runner
 * works, the `@` alias reaches `src/`, the jsdom stubs are installed, the empty app mounts,
 * and axe runs over it.
 *
 * It asserts NO product behavior. The units after S1 do that.
 */
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { App } from '@/app/App';
import { SHELL_IDS, resolveMount } from '@/main';
import { AXE_IN_JSDOM } from './axe';
import { downloads, navigations, objectUrls } from './setup';

describe('scaffold: the toolchain', () => {
  it('runs TypeScript tests under jsdom', () => {
    const node = document.createElement('div');
    node.textContent = 'ok';
    document.body.append(node);
    expect(node.isConnected).toBe(true);
    expect(document.body.textContent).toContain('ok');
  });

  it('resolves the @ alias to src/', () => {
    expect(typeof App).toBe('function');
    expect(SHELL_IDS).toEqual(['view', 'topbar', 'toasts']);
  });
});

describe('scaffold: jsdom stubs', () => {
  it('stubs the globals jsdom omits outright', () => {
    expect(typeof window.matchMedia).toBe('function');
    expect(window.matchMedia('(prefers-reduced-motion: reduce)').matches).toBe(false);
    expect(typeof ResizeObserver).toBe('function');
    expect(typeof URL.createObjectURL).toBe('function');
    expect(typeof URL.revokeObjectURL).toBe('function');
    expect(typeof window.open).toBe('function');
    expect(typeof window.scrollTo).toBe('function');
    // A UMD global from the vendored tree. Absent, the render idiom returns the raw text.
    expect(typeof (window as unknown as { renderMathInElement: unknown }).renderMathInElement)
      .toBe('function');
  });

  it('hands out and records object URLs', () => {
    const url = URL.createObjectURL(new Blob(['x']));
    expect(url).toMatch(/^blob:mock\//);
    expect(objectUrls).toContain(url);
  });

  it('gives the export a seam for the download anchor', () => {
    // jsdom implements a.click() as a navigation attempt: a no-op plus an unattributable
    // stderr from a timer. The helper also removes the anchor synchronously, so the href
    // must be captured at CALL time or there is nothing left to assert.
    const a = document.createElement('a');
    a.href = '/api/export';
    a.download = 'cadus-export.jsonl';
    a.click();
    expect(downloads).toEqual([
      { href: `${location.origin}/api/export`, download: 'cadus-export.jsonl' },
    ]);
  });

  it('gives the OAuth start a seam for its redirect', () => {
    // The OAuth start assigns window.location.href. In stock jsdom that is a silent no-op.
    window.location.href = '/api/auth/oauth/google/start';
    expect(navigations).toEqual(['/api/auth/oauth/google/start']);
  });

  it('seeds the document shell index.html provides', () => {
    expect(document.getElementById('topbar')).toBeTruthy();
    expect(document.getElementById('view')).toBeTruthy();
    expect(document.getElementById('toasts')?.getAttribute('aria-live')).toBe('polite');
  });

  it('configures the React act() environment', () => {
    // Without this React warns and does not schedule reliably, so a React test passes for
    // the wrong reason — worse than a test that fails.
    expect((globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT)
      .toBe(true);
  });
});

describe('scaffold: the empty app', () => {
  it('mounts and renders its heading', () => {
    render(<App />);
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Cadus');
  });

  it('reports zero axe violations', async () => {
    const { container } = render(<App />);
    expect(await axe(container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('resolves the mount point from the shell', () => {
    expect(resolveMount(document).id).toBe('view');
  });

  it('names every missing shell id instead of failing three times over', () => {
    document.body.innerHTML = '<main id="view"></main>';
    expect(() => resolveMount(document)).toThrow('index.html is missing #topbar, #toasts');
  });
});
