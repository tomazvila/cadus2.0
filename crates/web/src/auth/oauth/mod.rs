//! The OAuth mechanics: providers, PKCE, the handshake record, and the
//! provider-verified identity.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, rows "OAuth
//! providers" and "OAuth handshake"; section 10, rows "OAuth" and "Lifetimes"
//! (1.0 `authn/oauth.py`).
//!
//! This module is pure. It builds strings, it reads JSON documents, and it makes
//! no call of its own. The one exception is the entropy that
//! [`Handshake::fresh`] draws for the `state` and the PKCE verifier.
//!
//! # What stops a forged callback
//!
//! No `id_token` is read anywhere, so no JWT parse and no JWKS fetch belongs
//! here. Three things carry the whole guard:
//!
//! 1. **PKCE S256.** The authorize request carries `sha256(code_verifier)`, and
//!    the token exchange carries the verifier. The verifier never leaves the
//!    handshake cookie, so a stolen `code` alone redeems nothing.
//! 2. **The `state` compare.** [`Handshake::fresh`] mints one `state` per
//!    sign-in and binds the provider name to it. The callback compares the query
//!    value against the cookie value in constant time with
//!    [`crate::auth::token::tokens_equal`].
//! 3. **The identity comes from the provider API**, read with the access token
//!    of this exchange, never from a field of the callback query.
//!
//! # The transport port (R4)
//!
//! `cadus-web` opens no socket. `tests/purity.rs` reads the resolved dependency
//! graph and the source of this crate, and it refuses an outbound HTTP client in
//! either. The token exchange still needs one call, so this module states the
//! call as data — [`ProviderRequest`] and [`ProviderResponse`] — and takes the
//! caller's [`ProviderTransport`] to run it.
//!
//! A deployment installs the transport in [`OAuthConfig`]. A configuration with
//! credentials and NO transport serves no OAuth at all: [`OAuthConfig::enabled`]
//! is false, and both routes answer `404 not_found`. See
//! [`crate::auth::oauth_routes`] for why that is the safe default.

use std::sync::Arc;

mod exchange;
mod handshake;

pub use exchange::{
    Identity, OAuthFailure, ProviderRequest, ProviderResponse, ProviderTransport, TransportError,
    access_token, exchange_and_fetch_identity, identity_from,
};
pub use handshake::{
    Handshake, STATE_BYTES, VERIFIER_BYTES, authorize_url, code_challenge_s256, decode_handshake,
    encode_handshake, safe_next,
};

/// The provider name of Google sign-in.
pub const GOOGLE: &str = "google";

/// The provider name of GitHub sign-in.
pub const GITHUB: &str = "github";

/// The name of the short-lived handshake cookie.
///
/// It carries no `__Host-` prefix on purpose: the prefix demands `Path=/`, and
/// this cookie is scoped to [`HANDSHAKE_PATH`] so no ordinary API request ever
/// carries it.
pub const HANDSHAKE_COOKIE: &str = "cadus_oauth_handshake";

/// The path the handshake cookie is scoped to. It is the prefix of both OAuth
/// routes.
pub const HANDSHAKE_PATH: &str = "/api/auth/oauth";

/// The PKCE challenge method. S256 only; `plain` is never sent.
pub const CODE_CHALLENGE_METHOD: &str = "S256";

/// The redirect target when the handshake carries no usable `next`.
pub const DEFAULT_NEXT: &str = "/";

/// The bytes that [`safe_next`] lets through, before it also drops a backslash.
///
/// The range is the printable ASCII set with the space removed. The floor drops
/// every control byte and the space, and the ceiling drops `0x7f`, which a
/// `Location` header value cannot carry.
pub const SAFE_NEXT_BYTES: std::ops::RangeInclusive<u8> = 0x21..=0x7e;

/// The `Accept` of the token exchange.
pub const JSON_ACCEPT: &str = "application/json";

/// One provider and the endpoints it is reached by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    /// The name in the route path and in `oauth_accounts.provider`.
    pub name: &'static str,
    /// Where the browser is sent to consent.
    pub authorize_url: &'static str,
    /// Where the `code` is exchanged for an access token.
    pub token_url: &'static str,
    /// The scope string of the authorize request.
    pub scope: &'static str,
    /// The `Accept` of the identity reads.
    pub api_accept: &'static str,
    /// The identity documents to read, in order.
    pub identity_urls: &'static [&'static str],
}

/// Google, through the OIDC userinfo endpoint.
///
/// The endpoints are written out and never discovered at run time. The
/// `.well-known` document holds these same values, and a discovery fetch would
/// add one round trip to every sign-in.
pub const GOOGLE_PROVIDER: Provider = Provider {
    name: GOOGLE,
    authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    scope: "openid email profile",
    api_accept: JSON_ACCEPT,
    identity_urls: &["https://openidconnect.googleapis.com/v1/userinfo"],
};

/// GitHub. `/user` gives the stable numeric id, `/user/emails` gives the primary
/// address and whether GitHub verified it.
pub const GITHUB_PROVIDER: Provider = Provider {
    name: GITHUB,
    authorize_url: "https://github.com/login/oauth/authorize",
    token_url: "https://github.com/login/oauth/access_token",
    scope: "read:user user:email",
    api_accept: "application/vnd.github+json",
    identity_urls: &[
        "https://api.github.com/user",
        "https://api.github.com/user/emails",
    ],
};

/// The two providers, in the order the sign-in page lists them.
pub const PROVIDERS: [Provider; 2] = [GOOGLE_PROVIDER, GITHUB_PROVIDER];

/// The provider that `name` names, or `None`.
#[must_use]
pub fn provider(name: &str) -> Option<Provider> {
    PROVIDERS.into_iter().find(|entry| entry.name == name)
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// The client id and secret of one provider.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    /// The public client id.
    pub client_id: String,
    /// The client secret. It reaches the token endpoint and nothing else.
    pub client_secret: String,
}

impl std::fmt::Debug for Credentials {
    /// Print the id and redact the secret. A `Debug` of a configuration must not
    /// put a client secret into a log line.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .finish()
    }
}

/// The environment variable that holds the Google client id.
pub const GOOGLE_ID_VAR: &str = "OAUTH_GOOGLE_CLIENT_ID";

/// The environment variable that holds the Google client secret.
pub const GOOGLE_SECRET_VAR: &str = "OAUTH_GOOGLE_CLIENT_SECRET";

/// The environment variable that holds the GitHub client id.
pub const GITHUB_ID_VAR: &str = "OAUTH_GITHUB_CLIENT_ID";

/// The environment variable that holds the GitHub client secret.
pub const GITHUB_SECRET_VAR: &str = "OAUTH_GITHUB_CLIENT_SECRET";

/// The environment variable that pins the external origin of the callback.
pub const REDIRECT_BASE_VAR: &str = "OAUTH_REDIRECT_BASE_URL";

/// The OAuth configuration of one deployment.
#[derive(Clone, Default)]
pub struct OAuthConfig {
    /// The Google credentials, when both halves are set.
    pub google: Option<Credentials>,
    /// The GitHub credentials, when both halves are set.
    pub github: Option<Credentials>,
    /// The external origin of the callback, as `scheme://host`. `None` falls
    /// back to the origin the request resolved to.
    pub redirect_base: Option<String>,
    /// The transport that runs the provider calls. `None` serves no OAuth.
    pub transport: Option<Arc<dyn ProviderTransport>>,
}

impl std::fmt::Debug for OAuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthConfig")
            .field("google", &self.google)
            .field("github", &self.github)
            .field("redirect_base", &self.redirect_base)
            .field("transport", &self.transport.is_some())
            .finish()
    }
}

impl OAuthConfig {
    /// Read the credentials and the redirect base from a lookup function.
    ///
    /// The function takes the lookup instead of reading the environment, so a
    /// test drives every branch with a map and the process reads the environment
    /// once, in the binary. A provider counts as configured only when BOTH
    /// halves are set and neither is blank.
    #[must_use]
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        let pair = |id: &str, secret: &str| -> Option<Credentials> {
            let client_id = get(id).filter(|value| !value.trim().is_empty())?;
            let client_secret = get(secret).filter(|value| !value.trim().is_empty())?;
            Some(Credentials {
                client_id,
                client_secret,
            })
        };
        Self {
            google: pair(GOOGLE_ID_VAR, GOOGLE_SECRET_VAR),
            github: pair(GITHUB_ID_VAR, GITHUB_SECRET_VAR),
            redirect_base: get(REDIRECT_BASE_VAR).filter(|value| !value.trim().is_empty()),
            transport: None,
        }
    }

    /// The same configuration with a transport installed.
    #[must_use]
    pub fn with_transport(mut self, transport: Arc<dyn ProviderTransport>) -> Self {
        self.transport = Some(transport);
        self
    }

    /// The credentials of `name`, whether or not a transport is installed.
    #[must_use]
    pub fn credentials(&self, name: &str) -> Option<&Credentials> {
        match name {
            GOOGLE => self.google.as_ref(),
            GITHUB => self.github.as_ref(),
            _ => None,
        }
    }

    /// The provider and its credentials, when this deployment serves `name`.
    ///
    /// A provider is served only when it is a known provider, both credential
    /// halves are set, AND a transport is installed. The transport is part of
    /// the test on purpose: without one the callback cannot reach the provider,
    /// and a route that starts a sign-in it cannot finish is worse than a route
    /// that is not there.
    #[must_use]
    pub fn enabled(&self, name: &str) -> Option<(Provider, &Credentials)> {
        self.transport.as_ref()?;
        let found = provider(name)?;
        let credentials = self.credentials(name)?;
        Some((found, credentials))
    }

    /// The provider, its credentials, and the transport, when this deployment
    /// serves `name`. It is [`Self::enabled`] with the transport in hand, so a
    /// route that passed it never asks for the transport a second time.
    pub(crate) fn served(
        &self,
        name: &str,
    ) -> Option<(Provider, &Credentials, Arc<dyn ProviderTransport>)> {
        let transport = Arc::clone(self.transport.as_ref()?);
        let (provider, credentials) = self.enabled(name)?;
        Some((provider, credentials, transport))
    }

    /// The names this deployment serves, in [`PROVIDERS`] order.
    #[must_use]
    pub fn enabled_names(&self) -> Vec<&'static str> {
        PROVIDERS
            .iter()
            .filter(|entry| self.enabled(entry.name).is_some())
            .map(|entry| entry.name)
            .collect()
    }
}

/// The `redirect_uri` of one provider's callback.
///
/// The string must be byte-identical at the start and at the callback, and it
/// must match the provider console, so both routes build it here. `base` is the
/// pinned external origin, or the origin the request resolved to.
#[must_use]
pub fn callback_redirect_uri(name: &str, base: &str) -> String {
    format!(
        "{}{HANDSHAKE_PATH}/{name}/callback",
        base.trim_end_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;

    use super::*;

    /// A transport that answers nothing. The test below never calls it.
    struct NoTransport;

    impl ProviderTransport for NoTransport {
        fn fetch<'a>(
            &'a self,
            request: ProviderRequest,
        ) -> Pin<Box<dyn Future<Output = Result<ProviderResponse, TransportError>> + Send + 'a>>
        {
            Box::pin(async move {
                Err(TransportError {
                    reason: format!("no transport for {}", request.url),
                })
            })
        }
    }

    /// `with_transport` installs the transport, and a name that is no provider
    /// has no credentials and is not served.
    #[test]
    fn with_transport_installs_the_transport_and_an_unknown_name_is_not_served() {
        let config = OAuthConfig::from_env(|name| match name {
            GOOGLE_ID_VAR => Some("id".to_string()),
            GOOGLE_SECRET_VAR => Some("secret".to_string()),
            _ => None,
        })
        .with_transport(Arc::new(NoTransport));
        assert!(config.transport.is_some());
        assert!(config.credentials("nope").is_none());
        assert!(config.served("nope").is_none());
        assert!(config.served(GOOGLE).is_some());
        assert_eq!(config.enabled_names(), vec![GOOGLE]);
    }
}
