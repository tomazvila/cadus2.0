/**
 * The application root.
 *
 * S1 delivers the scaffold, so this renders the empty shell the gate runs against. S4
 * replaces the body with the app shell and the topbar; S2 gives it an API client; the view
 * router arrives with the first screens.
 *
 * It stays a real component, not a placeholder string: the a11y gate, the axe pass, and the
 * `console.error` trap all need a tree to mount.
 */
export function App() {
  return (
    <section className="card">
      <h1>Cadus</h1>
      <p className="muted">The scaffold is up. The screens arrive with the units after it.</p>
    </section>
  );
}
