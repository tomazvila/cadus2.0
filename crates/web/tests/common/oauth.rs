//! M5 U5: the fake OAuth provider and the OAuth configurations.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::auth::oauth::{
    Credentials, OAuthConfig, ProviderRequest, ProviderResponse, ProviderTransport, TransportError,
};
use cadus_web::auth::password::Argon2Profile;
use cadus_web::{AppState, create_app};
use sqlx::types::Uuid;

use super::Answer;

/// The external origin the OAuth tests build the `redirect_uri` on.
pub const TEST_ORIGIN: &str = "https://tutor.example";

/// The Google client id the OAuth tests configure.
pub const GOOGLE_CLIENT_ID: &str = "u5-google-client-id";

/// The Google client secret the OAuth tests configure.
pub const GOOGLE_CLIENT_SECRET: &str = "u5-google-client-secret";

/// The GitHub client id the OAuth tests configure.
pub const GITHUB_CLIENT_ID: &str = "u5-github-client-id";

/// The GitHub client secret the OAuth tests configure.
pub const GITHUB_CLIENT_SECRET: &str = "u5-github-client-secret";

/// A provider that answers from a table instead of from a socket.
///
/// `cadus-web` opens no socket (R4, `tests/purity.rs`), so the production code
/// states each provider call as a `ProviderRequest` and hands it to an installed
/// transport. This fake IS that transport: it answers the URL of the request
/// from a table of canned pairs, and it records every request it saw. A test
/// then asserts on the exact bytes the service sent, which a socket server would
/// only hide behind one more parse.
///
/// An unknown URL is a transport error, so a test that forgets to stub an
/// endpoint fails instead of passing on a default.
pub struct FakeProvider {
    answers: Mutex<HashMap<String, (u16, Vec<u8>)>>,
    seen: Mutex<Vec<ProviderRequest>>,
}

impl FakeProvider {
    /// A provider with no canned answer.
    pub fn new() -> Self {
        Self {
            answers: Mutex::new(HashMap::new()),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Answer `url` with `status` and the bytes of `body`.
    pub fn answer(self, url: &str, status: u16, body: &str) -> Self {
        self.answers
            .lock()
            .unwrap()
            .insert(url.to_string(), (status, body.as_bytes().to_vec()));
        self
    }

    /// Every request the service made, in order.
    pub fn seen(&self) -> Vec<ProviderRequest> {
        self.seen.lock().unwrap().clone()
    }
}

impl ProviderTransport for FakeProvider {
    fn fetch<'a>(
        &'a self,
        request: ProviderRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProviderResponse, TransportError>> + Send + 'a>> {
        let found = self.answers.lock().unwrap().get(&request.url).cloned();
        let url = request.url.clone();
        self.seen.lock().unwrap().push(request);
        Box::pin(async move {
            match found {
                Some((status, body)) => Ok(ProviderResponse { status, body }),
                None => Err(TransportError {
                    reason: format!("the fake provider has no answer for {url}"),
                }),
            }
        })
    }
}

/// A Google-only configuration on `transport`.
pub fn google_config(transport: Arc<FakeProvider>) -> OAuthConfig {
    OAuthConfig {
        google: Some(Credentials {
            client_id: GOOGLE_CLIENT_ID.to_string(),
            client_secret: GOOGLE_CLIENT_SECRET.to_string(),
        }),
        github: None,
        redirect_base: Some(TEST_ORIGIN.to_string()),
        transport: Some(transport),
    }
}

/// A GitHub-only configuration on `transport`.
pub fn github_config(transport: Arc<FakeProvider>) -> OAuthConfig {
    OAuthConfig {
        google: None,
        github: Some(Credentials {
            client_id: GITHUB_CLIENT_ID.to_string(),
            client_secret: GITHUB_CLIENT_SECRET.to_string(),
        }),
        redirect_base: Some(TEST_ORIGIN.to_string()),
        transport: Some(transport),
    }
}

/// The application under test with `oauth` installed.
pub fn oauth_app(db: &TestDb, oauth: OAuthConfig) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_argon2(Argon2Profile::TEST)
            .with_oauth(oauth),
    )
}

/// A `GET` of `path` that carries `cookie` as the whole `Cookie` header.
pub fn get_with_cookie(path: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

/// The `Set-Cookie` headers of an answer, in order.
pub fn cookies_of(answer: &Answer) -> Vec<String> {
    answer
        .headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_string)
        .collect()
}

/// The `Location` header of an answer.
pub fn location_of(answer: &Answer) -> String {
    answer
        .headers
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// How many `oauth_accounts` rows one account has.
pub async fn oauth_link_count(db: &TestDb, user: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM oauth_accounts WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// How many accounts this database holds.
pub async fn user_count(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&db.admin)
        .await
        .unwrap()
}
