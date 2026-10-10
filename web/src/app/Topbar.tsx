/**
 * The persistent top bar.
 *
 * It PORTALS into the `#topbar` element index.html ships, rather than rendering inside the
 * React root. The root owns `#view`; the bar and the toast region are siblings of it in the
 * document, and all three exist before the first render. For `#toasts` that is the whole
 * point — a live region announces a CHANGE, so one that mounts with its content announces
 * nothing. For the bar it is structural: `<header id="topbar">` is the banner landmark, and
 * a landmark that appears one tick after the page does moves under a screen reader.
 *
 * WHAT THE BAR OFFERS, and why it is this short: it is the one way out of every screen, so
 * it carries the way home, the map, and the way out of the session. Nothing else. Spec
 * section 4.2 asks for high signal and no chrome; a bar that grows a menu per screen is the
 * opposite.
 */
import { createPortal } from 'react-dom';
import '../styles/fix-shell.css';
import { BrandMark } from '@/components/primitives';
import type { User } from '@/api/types';

export interface TopbarProps {
  /** The signed-in account, or `null` while signed out. S6 supplies the real value. */
  user: User | null;
  /** Demo mode has no session, so it shows the DEMO badge in place of the user cluster. */
  demo: boolean;
  onHome: () => void;
  onMap: () => void;
  onLogout: () => void;
  /** The name of the screen on: the brand marks the dashboard, the Map button the map. */
  view?: string;
  /** True while a lesson, quiz or placement is on screen: only the brand stays. */
  focus?: boolean;
}

export function Topbar({ user, demo, onHome, onMap, onLogout, view, focus = false }: TopbarProps) {
  const host = document.getElementById('topbar');
  if (!host) return null;

  // The quiet Map affordance — offered while signed in, and in demo.
  const mapLink = (
    <button
      type="button"
      className="topbar-link"
      title="Curriculum map"
      aria-current={view === 'map' ? 'page' : undefined}
      onClick={onMap}
    >
      <svg className="topbar-glyph" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true" focusable="false">
        <path
          d="M1.5 3.5 5.5 2l5 1.5 4-1.5v10.5l-4 1.5-5-1.5-4 1.5zM5.5 2v10.5M10.5 3.5V14"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.3"
          strokeLinejoin="round"
        />
      </svg>
      Map
    </button>
  );

  return createPortal(
    <>
      <nav className="topbar-nav" aria-label="Main">
      <button
        type="button"
        className="brand"
        title="Dashboard"
        aria-current={view === 'dashboard' ? 'page' : undefined}
        onClick={onHome}
      >
        <BrandMark id="brand-mark-top" />
        Cadus
      </button>
      {demo ? (
        <>
          {focus ? null : mapLink}
          <span className="demo-badge">DEMO</span>
        </>
      ) : user ? (
        <>
          {focus ? null : mapLink}
          <div className="topbar-user">
            {/* The address is mono, because it is an identifier, and it truncates rather
                than pushing Log out off the bar on a narrow screen. `title` keeps the full
                address reachable. */}
            <span className="user-email mono" title={user.email}>{user.email}</span>
            {focus ? null : (
              <button type="button" className="btn btn-ghost logout-btn" onClick={onLogout}>
                Log out
              </button>
            )}
          </div>
        </>
      ) : null}
      </nav>
    </>,
    host,
  );
}
