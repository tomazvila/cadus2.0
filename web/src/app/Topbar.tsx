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
}

export function Topbar({ user, demo, onHome, onMap, onLogout }: TopbarProps) {
  const host = document.getElementById('topbar');
  if (!host) return null;

  // The quiet Map affordance — offered while signed in, and in demo.
  const mapLink = (
    <button type="button" className="topbar-link" title="Curriculum map" onClick={onMap}>
      Map
    </button>
  );

  return createPortal(
    <>
      <button type="button" className="brand" title="Dashboard" onClick={onHome}>
        <BrandMark id="brand-mark-top" />
        Cadus
      </button>
      {demo ? (
        <>
          {mapLink}
          <span className="demo-badge">DEMO</span>
        </>
      ) : user ? (
        <>
          {mapLink}
          <div className="topbar-user">
            {/* The address is mono, because it is an identifier, and it truncates rather
                than pushing Log out off the bar on a narrow screen. `title` keeps the full
                address reachable. */}
            <span className="user-email mono" title={user.email}>{user.email}</span>
            <button type="button" className="btn btn-ghost logout-btn" onClick={onLogout}>
              Log out
            </button>
          </div>
        </>
      ) : null}
    </>,
    host,
  );
}
