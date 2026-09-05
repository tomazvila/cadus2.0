/**
 * The auth gate: seven modes on one card.
 *
 * AUTH-inline SHAPES THIS WHOLE FILE. Every call here goes STRAIGHT to the client and never
 * through `useCall`. `invalid_credentials` is a 401, exactly as an expired session is, and
 * `useCall` routes a 401 to sign-in with "Your session has expired". A learner who mistypes
 * a password on the sign-in screen is then thrown to the screen they are already on, with a
 * message about a session they never had. So: these screens read `ApiError.code` and put the
 * line beside the field (trap T14).
 *
 * THE INPUTS ARE CONTROLLED, unlike the answer field of S5. They are small, they sit on no
 * 1 Hz render path, and the typed email surviving the login-signup-forgot toggles is state.
 *
 * AUTH-7. The OAuth buttons render only for a provider the service advertises. 1.0 read the
 * list off `/api/health`; 2.0 answers `{"ok":true}` there and publishes
 * `GET /api/auth/oauth/providers` instead (`src/api/types.ts`), so the client method is
 * `oauthProviders()`. An unconfigured provider is absent from that list, the list is empty,
 * and no third-party button reaches the page.
 */
import { useEffect, useRef, useState } from 'react';
import { toast } from '@/app/toast';
import { BrandMark } from '@/components/primitives';
import type { ApiClient, User } from '@/api';

export type AuthMode = 'login' | 'signup' | 'forgot' | 'reset' | 'check-email' | 'sent';

/** The shortest password the service accepts. Checked here to save a round trip. */
const MIN_PASSWORD_LENGTH = 8;

const TAGLINE = 'Practice, on cadence — learn by doing, one problem at a time.';

const MODE_TITLE: Record<AuthMode, string> = {
  login: 'sign in',
  signup: 'create account',
  forgot: 'reset your password',
  reset: 'choose a new password',
  'check-email': 'check your email',
  sent: 'reset your password',
};

/**
 * The envelope code, as one actionable line.
 *
 * The code is the input, not the status: a 401 here means the credentials, and the message
 * says so. A code this list does not name falls back to the server's own message, so a new
 * code reaches the learner as prose instead of as silence.
 */
export function messageFor(err: AuthFailure | null): string {
  switch (err?.code) {
    case 'invalid_credentials':
      return 'Incorrect email or password.';
    case 'weak_password':
      return err.message || 'Choose a stronger password (at least 8 characters).';
    case 'invalid_token':
      return 'That link is invalid or has expired — request a new one below.';
    case 'rate_limited':
      return 'Too many attempts. Please wait a minute, then try again.';
    case 'network':
      return err.message || 'Network error — check your connection and try again.';
    default:
      return err?.message || 'Something went wrong. Please try again.';
  }
}

/** What a failed auth call carries: an `ApiError`, or a foreign throw with a message. */
interface AuthFailure {
  code?: string | undefined;
  message?: string | undefined;
}

/** `google` reads as `Google` on the button. The service names the provider in lower case. */
export function providerLabel(id: string): string {
  return id.charAt(0).toUpperCase() + id.slice(1);
}

export interface AuthProps {
  /** The API client. Passed in, never imported: the demo swap stays one object. */
  api: ApiClient;
  /** `reset` arrives from a `?reset=<token>` email link; the rest from the route. */
  mode?: AuthMode;
  /** The single-use reset token, for `reset` mode. Absent, or undefined, in the others. */
  token?: string | undefined;
  /** Called with the account after a successful sign-in. */
  onSignedIn: (user: User) => void;
}

export function Auth({ api, mode: initialMode = 'login', token, onSignedIn }: AuthProps) {
  const [mode, setMode] = useState<AuthMode>(initialMode);
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [providers, setProviders] = useState<string[]>([]);
  const [sentTo, setSentTo] = useState('');

  const emailRef = useRef<HTMLInputElement>(null);
  const passwordRef = useRef<HTMLInputElement>(null);

  // AUTH-7. Once per mount, not per mode change: the configured providers do not change
  // while the learner toggles between sign-in and sign-up. A failure leaves the list empty
  // and the page keeps email and password, which is the correct degradation.
  useEffect(() => {
    // A reply that lands after the view left writes state nobody renders, and a body the
    // wrapper could not parse is a probe that failed: the catch below covers both.
    void api
      .oauthProviders()
      .then((res) => {
        const list = res.providers;
        if (Array.isArray(list) && list.length) setProviders(list);
      })
      .catch(() => {
        /* no providers — email and password only */
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Focus the field the learner still has to fill. Only on a mode change: per keystroke it
  // would fight the caret.
  useEffect(() => {
    // Each card renders the field its mode focuses, so the refs name one.
    if (mode === 'reset') {
      passwordRef.current!.focus();
      return;
    }
    if (mode === 'login' || mode === 'signup') {
      (email ? passwordRef : emailRef).current!.focus();
      return;
    }
    if (mode === 'forgot') emailRef.current!.focus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode]);

  const run = async (fn: () => Promise<void>): Promise<void> => {
    setBusy(true);
    try {
      await fn();
    } finally {
      setBusy(false);
    }
  };

  /** The trimmed address a submit posts, or null once the empty-field line is on screen. */
  function takeEmail(e: React.FormEvent): string | null {
    e.preventDefault();
    const address = email.trim();
    setError('');
    if (address) return address;
    setError('Enter your email.');
    emailRef.current!.focus();
    return null;
  }

  function submitCredentials(e: React.FormEvent) {
    const address = takeEmail(e);
    if (!address) return;
    if (!password) {
      setError('Enter your password.');
      passwordRef.current!.focus();
      return;
    }
    if (mode === 'signup' && password.length < MIN_PASSWORD_LENGTH) {
      setError('Password must be at least 8 characters.');
      passwordRef.current!.focus();
      return;
    }

    void run(async () => {
      try {
        if (mode === 'login') {
          const res = await api.login(address, password);
          onSignedIn(res.user);
        } else {
          // Sign-up is non-enumerable: it signs nobody in and it answers the same object
          // for a new address and a registered one. The emailed link activates the account,
          // so the one correct next screen is "check your email".
          await api.signup(address, password);
          setSentTo(address);
          setMode('check-email');
        }
      } catch (err) {
        // AUTH-inline: the line lands beside the field. Nothing routes anywhere.
        setError(messageFor(err as AuthFailure));
        passwordRef.current!.focus();
      }
    });
  }

  function submitForgot(e: React.FormEvent) {
    const address = takeEmail(e);
    if (!address) return;
    void run(async () => {
      try {
        await api.forgotPassword(address);
        // The same confirmation for a known address and an unknown one — anti-enumeration.
        setSentTo(address);
        setMode('sent');
      } catch (err) {
        setError(messageFor(err as AuthFailure));
      }
    });
  }

  function submitReset(e: React.FormEvent) {
    e.preventDefault();
    setError('');
    if (password.length < MIN_PASSWORD_LENGTH) {
      setError('Password must be at least 8 characters.');
      passwordRef.current!.focus();
      return;
    }
    void run(async () => {
      try {
        // The reset card is on because a link carried a token, so the prop names one.
        await api.resetPassword(token!, password);
        toast('Password updated — sign in with your new password.', { kind: 'info' });
        setPassword('');
        setMode('login');
      } catch (err) {
        setError(messageFor(err as AuthFailure));
        passwordRef.current!.focus();
      }
    });
  }

  const header = (
    <>
      <div className="auth-brand">
        <BrandMark id="brand-mark-auth" />
        <span className="brand-lg">Cadus</span>
      </div>
      <p className="muted auth-tagline">{TAGLINE}</p>
      <p className="auth-mode">{MODE_TITLE[mode]}</p>
    </>
  );

  // Mounted once and only FILLED. `role="alert"` announces a CHANGE of content, so a node
  // that mounts together with its message announces nothing.
  const errorBox = (
    <p className="field-error" role="alert" hidden={!error}>
      {error}
    </p>
  );

  const backToSignIn = (
    <p className="auth-toggle muted">
      <button
        type="button"
        className="link-btn"
        // The password is cleared too. A live credential left in a form the learner walked
        // away from is worse than the small inconsistency with the sign-in toggle.
        onClick={() => {
          setError('');
          setPassword('');
          setMode('login');
        }}
      >
        Back to sign in
      </button>
    </p>
  );

  const emailField = (
    <>
      <label className="auth-label" htmlFor="auth-email">Email</label>
      <input
        ref={emailRef}
        id="auth-email"
        type="email"
        className="auth-input"
        name="email"
        placeholder="you@example.com"
        autoComplete="username"
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck={false}
        required
        value={email}
        onChange={(e) => setEmail(e.target.value)}
      />
    </>
  );

  if (mode === 'check-email') {
    return (
      <section className="auth-view">
        <form className="modal auth-card" noValidate onSubmit={(e) => e.preventDefault()}>
          {header}
          <p className="muted auth-help">
            Check your email for a link to finish creating your account. If an account with
            that email already exists, we sent a sign-in reminder instead.
          </p>
          <p className="auth-check-email-addr mono">{sentTo}</p>
          <button
            type="button"
            className="btn btn-ghost auth-submit"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                try {
                  await api.resendVerification(sentTo);
                  toast('Verification email re-sent.', { kind: 'success' });
                } catch {
                  toast('Could not resend right now — please try again shortly.', { kind: 'info' });
                }
              })
            }
          >
            Resend email
          </button>
          {backToSignIn}
        </form>
      </section>
    );
  }

  if (mode === 'sent') {
    return (
      <section className="auth-view">
        <form className="modal auth-card" noValidate onSubmit={(e) => e.preventDefault()}>
          {header}
          <p className="auth-help">
            If an account exists for {sentTo}, a password-reset link is on its way. It
            expires in 30 minutes.
          </p>
          <p className="muted auth-help">
            Look in your inbox and in your spam folder, then open the link to choose a new
            password.
          </p>
          {backToSignIn}
        </form>
      </section>
    );
  }

  if (mode === 'forgot') {
    return (
      <section className="auth-view">
        <form className="modal auth-card" noValidate onSubmit={submitForgot}>
          {header}
          <p className="muted auth-help">
            Enter your account email and we send a link to set a new password.
          </p>
          {emailField}
          {errorBox}
          <button type="submit" className="btn btn-primary auth-submit" disabled={busy}>
            Email me a reset link
          </button>
          {backToSignIn}
        </form>
      </section>
    );
  }

  if (mode === 'reset') {
    return (
      <section className="auth-view">
        <form className="modal auth-card" noValidate onSubmit={submitReset}>
          {header}
          <p className="muted auth-help">
            Choose a new password for your account. This link is single-use.
          </p>
          <label className="auth-label" htmlFor="auth-password">New password</label>
          <input
            ref={passwordRef}
            id="auth-password"
            type="password"
            className="auth-input"
            name="password"
            placeholder="New password (8+ characters)"
            autoComplete="new-password"
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          {errorBox}
          <button type="submit" className="btn btn-primary auth-submit" disabled={busy}>
            Set new password
          </button>
          {backToSignIn}
        </form>
      </section>
    );
  }

  const isLogin = mode === 'login';
  return (
    <section className="auth-view">
      <form className="modal auth-card" noValidate onSubmit={submitCredentials}>
        {header}
        {emailField}
        <label className="auth-label" htmlFor="auth-password">Password</label>
        <input
          ref={passwordRef}
          id="auth-password"
          type="password"
          className="auth-input"
          name="password"
          placeholder={isLogin ? 'Password' : 'Choose a password (8+ characters)'}
          autoComplete={isLogin ? 'current-password' : 'new-password'}
          required
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        {errorBox}
        <button type="submit" className="btn btn-primary auth-submit" disabled={busy}>
          {isLogin ? 'Sign in' : 'Create account'}
        </button>

        {isLogin ? (
          <p className="auth-forgot muted">
            <button
              type="button"
              className="link-btn"
              onClick={() => {
                setError('');
                setMode('forgot');
              }}
            >
              Forgot password?
            </button>
          </p>
        ) : null}

        {/* AUTH-7: an empty list renders no divider and no button. */}
        {providers.length ? (
          <>
            <div className="auth-divider"><span>or</span></div>
            {providers.map((id) => (
              <button
                key={id}
                type="button"
                className="btn oauth-btn"
                // A whole-page navigation, not a fetch: the provider answers with a
                // cross-origin redirect no `fetch` may follow.
                onClick={() => {
                  window.location.href = api.oauthStartUrl(id);
                }}
              >
                Continue with {providerLabel(id)}
              </button>
            ))}
          </>
        ) : null}

        <p className="auth-toggle muted">
          {isLogin ? 'New here? ' : 'Already have an account? '}
          <button
            type="button"
            className="link-btn"
            onClick={() => {
              // The typed email survives the toggle; the password field resets.
              setPassword('');
              setError('');
              setMode(isLogin ? 'signup' : 'login');
            }}
          >
            {isLogin ? 'Create an account' : 'Sign in'}
          </button>
        </p>
      </form>
    </section>
  );
}
