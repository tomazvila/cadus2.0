/**
 * The small visual primitives.
 *
 * Grouped in one file on purpose: each is three to eight lines, and eight component files
 * for that is decomposition for its own sake.
 *
 * Two shape rules the stylesheet depends on, and a rewrite must keep:
 *   * the ring label is `<strong>` over `<span>` — `app.css` sizes the number through the
 *     element type, so a `<span>` there silently loses the size and the line break;
 *   * the mark, the spinner, the tick and the cross are inline `<svg>`, because the
 *     spinner's animation and the mark's gradient are on the element.
 *
 * Every one of them is decorative. Each carries `aria-hidden="true"`, and the text beside it
 * carries the meaning — a tick with no sentence tells a screen reader nothing.
 */
import { useId } from 'react';
import { clamp01 } from '@/lib/format';

// The ring geometry and the percent readout of the dashboard clamp the same fraction, so
// `lib/format.ts` owns the one implementation. The re-export keeps the S4 import path.
export { clamp01 };

/**
 * The Cadus mark: four strokes on a widening beat — the spaced-repetition intervals —
 * painted with the accent to accent-2 gradient.
 *
 * The gradient `id` must be unique per instance, or two marks on one page collide and one
 * renders with the other's gradient. `useId()` gives that for free; a caller passes `id`
 * only for a stable snapshot.
 */
export function BrandMark({ id }: { id?: string }) {
  const generated = useId();
  const gradientId = id ?? `cadus-mark-${generated}`;
  return (
    <span className="brand-mark" aria-hidden="true">
      <svg viewBox="0 0 48 48" role="img" aria-hidden="true">
        <defs>
          <linearGradient id={gradientId} x1="0" y1="0" x2="1" y2="0">
            <stop offset="0" style={{ stopColor: 'var(--accent)' }} />
            <stop offset="1" style={{ stopColor: 'var(--accent-2)' }} />
          </linearGradient>
        </defs>
        <g stroke={`url(#${gradientId})`} strokeWidth="4.5" strokeLinecap="round">
          <line x1="8" y1="12" x2="8" y2="36" />
          <line x1="16" y1="12" x2="16" y2="36" />
          <line x1="26" y1="12" x2="26" y2="36" />
          <line x1="38" y1="12" x2="38" y2="36" />
        </g>
      </svg>
    </span>
  );
}

export function Spinner() {
  return (
    <span className="spinner" aria-hidden="true">
      <svg viewBox="0 0 24 24" width="18" height="18">
        <circle
          cx="12" cy="12" r="9" fill="none" stroke="currentColor"
          strokeWidth="3" strokeLinecap="round" strokeDasharray="42" strokeDashoffset="14"
        />
      </svg>
    </span>
  );
}

/** The wait state. The label is the announced part; the spinner beside it is decoration. */
export function LoadingBlock({ label = 'Loading…' }: { label?: string }) {
  return (
    <div className="loading">
      <Spinner />
      <span>{label}</span>
    </div>
  );
}

export function Chip({ children, className = '' }: { children: React.ReactNode; className?: string }) {
  return <span className={`chip ${className}`.trim()}>{children}</span>;
}

export function Stat({ value, label, className }: { value: string; label: string; className?: string | undefined }) {
  return (
    <div className={className ? `stat ${className}` : 'stat'}>
      <div className="stat-value">{value}</div>
      <div className="stat-label">{label}</div>
    </div>
  );
}

/** The progress ring. `label` and `sub` are centered inside it. */
export function Ring({ fraction, label = '', sub = '' }: { fraction: number; label?: string; sub?: string }) {
  const R = 54;
  const C = 2 * Math.PI * R;
  const off = C * (1 - clamp01(fraction));
  return (
    <div className="ring">
      <svg viewBox="0 0 128 128" width="120" height="120" role="img" aria-hidden="true">
        <circle className="ring-bg" cx="64" cy="64" r={R} fill="none" strokeWidth="11" />
        <circle
          className="ring-fg" cx="64" cy="64" r={R} fill="none" strokeWidth="11" strokeLinecap="round"
          strokeDasharray={C.toFixed(1)} strokeDashoffset={off.toFixed(1)} transform="rotate(-90 64 64)"
        />
      </svg>
      {/* `.ring-label strong` is an app.css selector — the element type is load-bearing. */}
      <div className="ring-label"><strong>{label}</strong><span>{sub}</span></div>
    </div>
  );
}

export const tickPath = 'M20 6L9 17l-5-5';
export const crossPath = 'M18 6L6 18M6 6l12 12';

export function Tick() {
  return (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
      <path d={tickPath} fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function Cross() {
  return (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
      <path d={crossPath} fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
    </svg>
  );
}
