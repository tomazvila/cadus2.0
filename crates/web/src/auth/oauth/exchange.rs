//! The transport port the callback calls through, the code exchange, and the
//! identity document it reads.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use super::handshake::encode_pairs;
use super::{Credentials, GITHUB, Handshake, JSON_ACCEPT, Provider};
use crate::auth::email::normalize_email;

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
