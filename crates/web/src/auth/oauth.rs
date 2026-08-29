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

use std::fmt::Write as _;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use base64ct::{Base64UrlUnpadded, Encoding};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::auth::email::normalize_email;
use crate::auth::token::{EntropyError, generate_token};

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

// ---------------------------------------------------------------------------
// The handshake record
// ---------------------------------------------------------------------------

/// The bytes behind the CSRF `state`: 192 bits.
pub const STATE_BYTES: usize = 24;

/// The bytes behind the PKCE verifier: 384 bits. RFC 7636 wants 43 to 128
/// characters, and 48 bytes give 64.
pub const VERIFIER_BYTES: usize = 48;

/// The sign-in record minted at the start route and spent at the callback.
///
/// The handshake cookie carries exactly this. It is unsigned, and it is safe
/// unsigned: the cookie is `HttpOnly`, so no script reads or writes it; `state`
/// is compared against the callback query, so a value an attacker chose matches
/// nothing they did not also start; and `next_url` is validated again on READ,
/// so a tampered target still cannot leave the site.
#[derive(Clone, PartialEq, Eq)]
pub struct Handshake {
    /// The provider this `state` was minted for.
    pub provider: String,
    /// The CSRF token, compared in constant time at the callback.
    pub state: String,
    /// The PKCE verifier, replayed at the token exchange.
    pub code_verifier: String,
    /// Where to send the browser after sign-in.
    pub next_url: String,
}

impl std::fmt::Debug for Handshake {
    /// Redact both secrets. A failing assertion prints this, and a `state` in a
    /// log line is a live CSRF token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handshake")
            .field("provider", &self.provider)
            .field("state", &"<redacted>")
            .field("code_verifier", &"<redacted>")
            .field("next_url", &self.next_url)
            .finish()
    }
}

impl Handshake {
    /// Mint a fresh handshake for `provider`.
    ///
    /// # Errors
    ///
    /// Returns [`EntropyError`] when the operating system gives no entropy. A
    /// predictable `state` or verifier defeats the whole guard, so the caller
    /// answers `500` and starts nothing.
    pub fn fresh(provider: &str, next_url: &str) -> Result<Self, EntropyError> {
        Ok(Self {
            provider: provider.to_string(),
            state: draw(STATE_BYTES)?,
            code_verifier: draw(VERIFIER_BYTES)?,
            next_url: safe_next(Some(next_url)),
        })
    }
}

/// Draw `bytes` of entropy in URL-safe text.
///
/// [`generate_token`] draws a fixed 32 bytes, and the two sizes here are 24 and
/// 48, so this function concatenates whole draws and cuts the text to the length
/// that `bytes` bytes of base64 occupy. Every character still comes from the
/// operating system.
fn draw(bytes: usize) -> Result<String, EntropyError> {
    let chars = bytes.div_ceil(3) * 4;
    let mut text = String::with_capacity(chars);
    while text.len() < chars {
        text.push_str(&generate_token()?);
    }
    text.truncate(chars);
    Ok(text)
}

/// A same-site relative target, or [`DEFAULT_NEXT`].
///
/// The accepted set is one path that holds three properties:
///
/// 1. the first byte is `/`;
/// 2. the second byte is neither `/` nor a backslash;
/// 3. no byte is below `0x21`, and no byte is a backslash.
///
/// Rule 2 closes the plain open redirect: a browser reads `//host` and `/\host`
/// as scheme-relative, so both leave the site. Rule 3 closes the same redirect
/// through one control byte. A browser removes every ASCII tab, LF, and CR from
/// a URL before it parses the URL (WHATWG URL, "remove all ASCII tab or
/// newline"), so a target of `/`, one tab, `/host` reaches the parser as
/// `//host` and leaves the site too. A backslash is a path separator to a
/// browser, so rule 3 refuses it in every position.
///
/// The callback runs this function again on the value it read from the cookie,
/// which is what lets the cookie stay unsigned.
#[must_use]
pub fn safe_next(next_url: Option<&str>) -> String {
    let Some(next) = next_url else {
        return DEFAULT_NEXT.to_string();
    };
    let bytes = next.as_bytes();
    let same_site = bytes.first() == Some(&b'/')
        && !matches!(bytes.get(1), Some(b'/' | b'\\'))
        && bytes.iter().all(|byte| *byte >= 0x21 && *byte != b'\\');
    if same_site {
        next.to_string()
    } else {
        DEFAULT_NEXT.to_string()
    }
}

/// Serialize a handshake into the cookie value.
///
/// The one-letter keys are a wire format. A cookie a running process wrote must
/// still decode after a restart, so a key is added or ignored, never renamed.
#[must_use]
pub fn encode_handshake(handshake: &Handshake) -> String {
    let payload = serde_json::json!({
        "p": handshake.provider,
        "s": handshake.state,
        "v": handshake.code_verifier,
        "next": handshake.next_url,
    });
    Base64UrlUnpadded::encode_string(payload.to_string().as_bytes())
}

/// Read a handshake back, or `None` for every unusable value.
///
/// `None` covers an absent cookie, a value that is not base64, a payload that is
/// not JSON, a payload that is not an object, and a payload missing any of the
/// three secrets. The callback answers all of them with one `400 oauth_error`.
#[must_use]
pub fn decode_handshake(value: Option<&str>) -> Option<Handshake> {
    let raw = Base64UrlUnpadded::decode_vec(value?.trim()).ok()?;
    let payload: Value = serde_json::from_slice(&raw).ok()?;
    let text = |key: &str| -> Option<String> {
        payload
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some(Handshake {
        provider: text("p")?,
        state: text("s")?,
        code_verifier: text("v")?,
        next_url: safe_next(payload.get("next").and_then(Value::as_str)),
    })
}

// ---------------------------------------------------------------------------
// The authorize redirect
// ---------------------------------------------------------------------------

/// Percent-encode one query or form component (RFC 3986 unreserved set).
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(byte) {
            out.push(char::from(*byte));
        } else {
            // The format writes two uppercase hex digits, so the answer stays
            // ASCII. A write into a `String` cannot fail.
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Join `pairs` into a query string or a form body.
fn encode_pairs(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// The PKCE S256 challenge of `verifier`: base64url, no padding, of its SHA-256.
#[must_use]
pub fn code_challenge_s256(verifier: &str) -> String {
    Base64UrlUnpadded::encode_string(&Sha256::digest(verifier.as_bytes()))
}

/// The provider URL the browser is sent to.
#[must_use]
pub fn authorize_url(
    provider: Provider,
    credentials: &Credentials,
    redirect_uri: &str,
    handshake: &Handshake,
) -> String {
    let challenge = code_challenge_s256(&handshake.code_verifier);
    let query = encode_pairs(&[
        ("response_type", "code"),
        ("client_id", &credentials.client_id),
        ("redirect_uri", redirect_uri),
        ("scope", provider.scope),
        ("state", &handshake.state),
        ("code_challenge", &challenge),
        ("code_challenge_method", CODE_CHALLENGE_METHOD),
    ]);
    format!("{}?{query}", provider.authorize_url)
}

// ---------------------------------------------------------------------------
// The transport port
// ---------------------------------------------------------------------------

/// One call to a provider, stated as data.
#[derive(Clone, PartialEq, Eq)]
pub struct ProviderRequest {
    /// `GET` or `POST`.
    pub method: &'static str,
    /// The absolute URL.
    pub url: String,
    /// The `application/x-www-form-urlencoded` body of a `POST`, already
    /// encoded.
    pub body: Option<String>,
    /// The access token of the `Authorization: Bearer` header.
    pub bearer: Option<String>,
    /// The `Accept` header.
    pub accept: &'static str,
}

impl std::fmt::Debug for ProviderRequest {
    /// Redact the body and the bearer. The body of the token exchange carries
    /// the client secret, and the bearer carries a live access token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("body", &self.body.as_ref().map(|_| "<redacted>"))
            .field("bearer", &self.bearer.as_ref().map(|_| "<redacted>"))
            .field("accept", &self.accept)
            .finish()
    }
}

/// What a provider answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResponse {
    /// The HTTP status.
    pub status: u16,
    /// The whole body.
    pub body: Vec<u8>,
}

/// The transport could not complete the call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError {
    /// What went wrong, for the log. It never reaches a client body.
    pub reason: String,
}

/// The seam that runs a [`ProviderRequest`].
///
/// `cadus-web` states the call and never makes it (R4). The deployment installs
/// an implementation through [`OAuthConfig::with_transport`], and a test installs
/// a fake provider.
///
/// The method gives a boxed future, because a trait with an `async fn` is not
/// object safe and this trait is held as `dyn`.
pub trait ProviderTransport: Send + Sync {
    /// Run one call.
    fn fetch<'a>(
        &'a self,
        request: ProviderRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProviderResponse, TransportError>> + Send + 'a>>;
}

// ---------------------------------------------------------------------------
// The exchange and the identity
// ---------------------------------------------------------------------------

/// A recoverable OAuth failure. The route answers `400 oauth_error`, never
/// `500`: a provider hiccup and a tampered callback are both bad requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthFailure {
    /// What went wrong, for the log.
    pub reason: String,
}

impl OAuthFailure {
    /// Build a failure from any text.
    fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

/// The provider-verified identity, reduced to what account linking needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The provider name.
    pub provider: String,
    /// The stable per-user id of the provider: Google `sub`, GitHub `id`.
    pub subject: String,
    /// The normalized address.
    pub email: String,
    /// The provider's own claim that it verified the address.
    pub email_verified: bool,
}

/// Coerce a provider's truth value: JSON `true` or the text `"true"`.
fn as_bool(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::String(text)) => text.trim().eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// Read one string field of a JSON object, numbers included.
///
/// GitHub answers a numeric `id` and Google answers a text `sub`, so the reader
/// takes both and gives text.
fn text_field(document: &Value, key: &str) -> String {
    match document.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

/// The access token of a token-endpoint answer.
///
/// # Errors
///
/// Returns [`OAuthFailure`] when the body is not JSON or carries no
/// `access_token`.
pub fn access_token(body: &[u8]) -> Result<String, OAuthFailure> {
    let payload: Value = serde_json::from_slice(body)
        .map_err(|_| OAuthFailure::new("the token endpoint answered a body that is not JSON"))?;
    let token = text_field(&payload, "access_token");
    if token.is_empty() {
        return Err(OAuthFailure::new(
            "the token endpoint answered no access_token",
        ));
    }
    Ok(token)
}

/// GitHub's primary address and whether GitHub verified it.
fn github_primary_email(emails: &Value) -> (String, bool) {
    let Some(entries) = emails.as_array() else {
        return (String::new(), false);
    };
    for entry in entries {
        if as_bool(entry.get("primary")) {
            let email = text_field(entry, "email");
            if !email.is_empty() {
                return (email, as_bool(entry.get("verified")));
            }
        }
    }
    (String::new(), false)
}

/// Build the identity from the documents that `provider.identity_urls` named.
///
/// # Errors
///
/// Returns [`OAuthFailure`] when a document is missing or carries no subject.
pub fn identity_from(provider: Provider, documents: &[Value]) -> Result<Identity, OAuthFailure> {
    let first = documents
        .first()
        .ok_or_else(|| OAuthFailure::new("the provider answered no identity document"))?;
    let (subject, email, verified) = if provider.name == GITHUB {
        let emails = documents
            .get(1)
            .ok_or_else(|| OAuthFailure::new("GitHub answered no address list"))?;
        let (email, verified) = github_primary_email(emails);
        (text_field(first, "id"), email, verified)
    } else {
        (
            text_field(first, "sub"),
            text_field(first, "email"),
            as_bool(first.get("email_verified")),
        )
    };
    if subject.is_empty() {
        return Err(OAuthFailure::new("the provider answered no account id"));
    }
    Ok(Identity {
        provider: provider.name.to_string(),
        subject,
        email: normalize_email(&email),
        email_verified: verified,
    })
}

/// Run one call and refuse a non-2xx answer.
async fn call(
    transport: &dyn ProviderTransport,
    request: ProviderRequest,
) -> Result<Vec<u8>, OAuthFailure> {
    let url = request.url.clone();
    let answer = transport
        .fetch(request)
        .await
        .map_err(|err| OAuthFailure::new(format!("the call to {url} failed: {}", err.reason)))?;
    if !(200..300).contains(&answer.status) {
        return Err(OAuthFailure::new(format!(
            "{url} answered {}",
            answer.status
        )));
    }
    Ok(answer.body)
}

/// Exchange `code` for an access token, then read the provider-verified
/// identity.
///
/// The identity comes from the provider API, read with the token this exchange
/// just minted, so a forged callback yields no identity at all.
///
/// # Errors
///
/// Returns [`OAuthFailure`] when a call fails, answers a non-2xx status, or
/// answers a document this module cannot read.
pub async fn exchange_and_fetch_identity(
    transport: &dyn ProviderTransport,
    provider: Provider,
    credentials: &Credentials,
    redirect_uri: &str,
    code: &str,
    handshake: &Handshake,
) -> Result<Identity, OAuthFailure> {
    let body = encode_pairs(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", &credentials.client_id),
        ("client_secret", &credentials.client_secret),
        ("code_verifier", &handshake.code_verifier),
    ]);
    let token_body = call(
        transport,
        ProviderRequest {
            method: "POST",
            url: provider.token_url.to_string(),
            body: Some(body),
            bearer: None,
            accept: JSON_ACCEPT,
        },
    )
    .await?;
    let access = access_token(&token_body)?;

    let mut documents = Vec::with_capacity(provider.identity_urls.len());
    for url in provider.identity_urls {
        let raw = call(
            transport,
            ProviderRequest {
                method: "GET",
                url: (*url).to_string(),
                body: None,
                bearer: Some(access.clone()),
                accept: provider.api_accept,
            },
        )
        .await?;
        documents.push(
            serde_json::from_slice(&raw).map_err(|_| {
                OAuthFailure::new(format!("{url} answered a body that is not JSON"))
            })?,
        );
    }
    identity_from(provider, &documents)
}
