/**
 * Global test setup.
 *
 * Every stub here exists because a line of shipping code needs it. jsdom is not a browser:
 * each one is either an API jsdom omits outright, or one it implements as a no-op that logs
 * "Not implemented" on every call.
 *
 * Two things are NOT fixable here, and a test must not pretend otherwise:
 *   * `getComputedStyle(...).getPropertyValue('--x')` returns '' in jsdom, so a token read
 *     yields an empty string. Assert the stylesheet text instead (the S4 contract test).
 *   * `offsetWidth` and `clientWidth` are permanently 0, so tooltip placement is vacuous in
 *     jsdom. Cover it as a pure function with injected dimensions.
 */
import { afterEach, beforeEach, expect, vi } from 'vitest';
import { cleanup } from '@testing-library/react';
import * as axeMatchers from 'vitest-axe/matchers';
import 'vitest-axe/extend-expect';
import { TextDecoder, TextEncoder } from 'node:util';
import { TransformStream } from 'node:stream/web';

expect.extend(axeMatchers);

// ---------------------------------------------------------------------------
// React act() environment.
//
// Without this flag React logs "The current testing environment is not configured to
// support act(...)" and does not schedule reliably. A test then passes for the WRONG
// reason, which is worse than a test that fails.
// ---------------------------------------------------------------------------
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// ---------------------------------------------------------------------------
// matchMedia — `prefers-color-scheme` selects the theme and `prefers-reduced-motion`
// strips every animation (spec section 4.5). Absent in jsdom. Configurable, so a test
// flips it.
// ---------------------------------------------------------------------------
const mediaState = new Map<string, boolean>();

/** Set what a media query reports, for the current test. */
export function setMedia(query: string, matches: boolean): void {
  mediaState.set(query, matches);
}

/**
 * Listeners are shared PER QUERY, not per `matchMedia()` call.
 *
 * A browser fires a change at every live MediaQueryList for the query; a per-call set means
 * a test that dispatches on its own handle reaches nobody, and the assertion fails as if
 * the component never subscribed.
 */
const mediaListeners = new Map<string, Set<(e: MediaQueryListEvent) => void>>();

/**
 * How many live listeners a query has.
 *
 * A leak test cannot spy on `removeEventListener`: `matchMedia()` hands back a NEW object
 * per call, so a spy on the test's handle never sees the component's. Counting the shared
 * set is the only way to prove a listener was released.
 */
export function mediaListenerCount(query: string): number {
  return mediaListeners.get(query)?.size ?? 0;
}

if (!window.matchMedia) {
  window.matchMedia = ((query: string) => {
    if (!mediaListeners.has(query)) mediaListeners.set(query, new Set());
    const listeners = mediaListeners.get(query)!;
    const mql = {
      get matches() { return mediaState.get(query) ?? false; },
      media: query,
      onchange: null,
      addEventListener: (_: string, fn: (e: MediaQueryListEvent) => void) => { listeners.add(fn); },
      removeEventListener: (_: string, fn: (e: MediaQueryListEvent) => void) => { listeners.delete(fn); },
      addListener: (fn: (e: MediaQueryListEvent) => void) => { listeners.add(fn); },
      removeListener: (fn: (e: MediaQueryListEvent) => void) => { listeners.delete(fn); },
      dispatchEvent: (e: Event) => {
        listeners.forEach((fn) => fn(e as MediaQueryListEvent));
        return true;
      },
    };
    return mql as unknown as MediaQueryList;
  }) as typeof window.matchMedia;
}

// ---------------------------------------------------------------------------
// ResizeObserver — the curriculum map constructs one at mount (S11). Absent in jsdom, so
// without this every map test throws before its first assertion.
// ---------------------------------------------------------------------------
if (!('ResizeObserver' in globalThis)) {
  class ResizeObserverStub implements ResizeObserver {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  }
  (globalThis as unknown as { ResizeObserver: typeof ResizeObserverStub }).ResizeObserver =
    ResizeObserverStub;
}

// ---------------------------------------------------------------------------
// Object URLs — the JSONL export (DEP-3, S7). Absent in jsdom.
// ---------------------------------------------------------------------------
let objectUrlSeq = 0;
/** Object URLs handed out, in order, so a test asserts create/revoke pairing. */
export const objectUrls: string[] = [];

if (!URL.createObjectURL) {
  URL.createObjectURL = vi.fn(() => {
    const url = `blob:mock/${++objectUrlSeq}`;
    objectUrls.push(url);
    return url;
  });
  URL.revokeObjectURL = vi.fn();
}

// ---------------------------------------------------------------------------
// The download anchor — the export helper of S2.
//
// jsdom implements `a.click()` as a navigation attempt: it emits "Not implemented:
// navigation" from a TIMER, that is, after the test already ended, as unattributable
// stderr. The helper also removes the anchor synchronously, so by the time it resolves
// there is nothing left in the DOM to assert against.
//
// The href and the download name are captured AT CALL TIME, before the node goes.
// ---------------------------------------------------------------------------
export const downloads: Array<{ href: string; download: string }> = [];

// ---------------------------------------------------------------------------
// window.location.href — the OAuth start is a plain assignment to it (AUTH-7, S6).
//
// In jsdom that assignment is a silent no-op plus another "Not implemented: navigation"
// error, so the redirect cannot be asserted at all. Replace the whole object with a
// recording proxy.
// ---------------------------------------------------------------------------
export const navigations: string[] = [];

// ---------------------------------------------------------------------------
// KaTeX globals — the render idiom reads `window.renderMathInElement` and returns the raw
// text when it is absent (spec section 4.3). index.html supplies it as a UMD global from
// the vendored tree.
//
// The default is a no-op that records its calls. S5 installs the REAL KaTeX in its own
// file to assert the string-render idiom; that asymmetry is deliberate.
// ---------------------------------------------------------------------------
export const mathRenderCalls: Element[] = [];

// Installed per test, NOT at module scope: `unstubGlobals: true` clears every stubbed
// global before each test, so a module-scope stub exists only for the first one.
function installGlobalStubs(): void {
  vi.stubGlobal('open', vi.fn());
  vi.stubGlobal('scrollTo', vi.fn());
  vi.stubGlobal('renderMathInElement', vi.fn((root: Element) => { mathRenderCalls.push(root); }));
}

// Re-installed per test for the same reason: `restoreMocks: true` restores every spy after
// each test, so a module-scope spy silently stops working after the first one — and the
// symptom is an empty `downloads` array that reads as "the code never downloaded".
function installAnchorSpy(): void {
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
    this: HTMLAnchorElement,
  ) {
    downloads.push({ href: this.href, download: this.download });
  });
}

let searchOverride: string | null = null;

/**
 * Set what `location.search` reports, for the boot-token tests of S6.
 *
 * It has to go through the proxy: `search` is a non-configurable accessor on the real
 * Location, so `Object.defineProperty(window.location, 'search', ...)` throws "Cannot
 * redefine property", and assigning it is a navigation jsdom does not implement. Cleared
 * between tests.
 */
export function setSearch(search: string): void {
  searchOverride = search;
}

const realLocation = window.location;
Object.defineProperty(window, 'location', {
  configurable: true,
  value: new Proxy(realLocation, {
    get: (target, prop) => {
      if (prop === 'search' && searchOverride !== null) return searchOverride;
      return prop === 'href' ? target.href : Reflect.get(target, prop);
    },
    set: (target, prop, value) => {
      if (prop === 'href') { navigations.push(String(value)); return true; }
      if (prop === 'search') { searchOverride = String(value); return true; }
      return Reflect.set(target, prop, value);
    },
  }),
});

// ---------------------------------------------------------------------------
// MSW v2 needs these Node globals restored under jsdom.
// ---------------------------------------------------------------------------
if (!globalThis.TextEncoder) Object.assign(globalThis, { TextEncoder, TextDecoder });
if (!('TransformStream' in globalThis)) Object.assign(globalThis, { TransformStream });

// ---------------------------------------------------------------------------
// React errors and warnings FAIL the test.
//
// 1.0 spent a whole commit issuing a network POST from React's render phase. React said so
// on every single run — "Cannot update a component while rendering a different component" —
// and the suite stayed green, because nothing asserted on console.error. A warning nobody
// fails on is a warning nobody reads.
//
// Anything genuinely expected must be declared with `allowConsoleError` in its own test.
// ---------------------------------------------------------------------------
const consoleErrors: string[] = [];
let allowed: RegExp[] = [];

/**
 * Declare a console.error this test EXPECTS.
 *
 * Use it only where the logged condition is the thing under test — a same-tick event
 * dispatch that sits outside `act()` on purpose, because wrapping it would flush a render
 * between the two events and hide the race being asserted.
 */
export function allowConsoleError(pattern: RegExp): void {
  allowed.push(pattern);
}

function trapConsole(): void {
  vi.spyOn(console, 'error').mockImplementation((...args: unknown[]) => {
    consoleErrors.push(args.map(String).join(' '));
  });
}

// ---------------------------------------------------------------------------
// Per-test hygiene. The toast store and the KaTeX memo are module-scope singletons by
// design, so nothing may survive a test.
// ---------------------------------------------------------------------------
beforeEach(() => {
  consoleErrors.length = 0;
  allowed = [];
  trapConsole();
  installAnchorSpy();
  installGlobalStubs();
  mediaState.clear();
  mathRenderCalls.length = 0;
  objectUrls.length = 0;
  downloads.length = 0;
  navigations.length = 0;
  searchOverride = null;
  document.head.innerHTML = '';
  // The document shell index.html provides. Boot resolves all three by id, so a bare body
  // fails every shell test for the wrong reason.
  document.body.innerHTML =
    '<header class="topbar" id="topbar"></header>'
    + '<main class="view" id="view"></main>'
    + '<div class="toasts" id="toasts" aria-live="polite"></div>';
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();

  // Deliberately AFTER cleanup, so an unmount-time warning is caught too.
  const errors = consoleErrors.filter((e) => !allowed.some((re) => re.test(e)));
  consoleErrors.length = 0;
  if (errors.length) {
    throw new Error(`console.error during this test:\n  ${errors.join('\n  ')}`);
  }
});
