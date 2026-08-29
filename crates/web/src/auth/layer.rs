//! The tenant layer: the one place a live credential becomes a bound tenant.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.3, "Cookie check",
//! and section 11, units U2, U4, U6, U7, U8, and U9. Ruling FIX-M5-C is
//! binding.
//!
//! [`crate::state::Tenant`] is an extension-only extractor: it reads
//! `parts.extensions` and consults no header. Before this layer, no code of
//! `cadus-web` wrote that extension, so every guarded route answered
//! `401 unauthorized` to a live session that the service itself minted. This
//! layer is the missing writer.
//!
//! The layer runs [`crate::auth::guard::current_user`], the one credential
//! reader of the crate, and then:
//!
//! - on success, it puts a `Tenant` AND an [`crate::auth::guard::Authed`] into
//!   the request extensions, and the guarded handler reads the `Tenant`;
//! - on a refusal, it passes the request through with NO `Tenant`, so the
//!   extractor answers `401 unauthorized` exactly as it did before.
//!
//! **The layer answers NO request of its own.** It adds a request extension or
//! it adds nothing, and every status code of the service still comes from a
//! route, from a fallback, or from the CSRF layer. That holds for a store
//! failure too: a database that refuses the session lookup gives the same `401`
//! as a bad cookie, and `store_call` already writes the cause to the log. A
//! layer that answered `500` here would put a new failure mode on `/api/health`,
//! on `/metrics`, and on every unguarded route, which is a worse trade than one
//! wrong status code on a tier that is down anyway.
//!
//! **The layer sits INSIDE the CSRF origin layer.** A cross-origin write with an
//! ambient cookie must be refused BEFORE the service reads that cookie as a
//! credential, so the CSRF layer runs first and this layer never sees the
//! request that CSRF rejects. The layer therefore also sits inside the
//! security-header layer and inside the request-metrics layer.
//!
//! **A request with no credential costs no query.** `current_user` selects the
//! credential first and gives `401` before it opens a statement, so the layer
//! adds nothing to `/api/health`, to `/metrics`, or to a sign-in POST (L1).

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;
use crate::auth::guard::current_user;
use crate::state::Tenant;

/// Bind the tenant of this request, or pass the request through unbound.
///
/// The module header gives the two outcomes and the layer order.
pub async fn tenant_layer(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    // The header borrow ends with this statement, so the block below borrows the
    // request again as mutable.
    let outcome = current_user(&state, request.headers()).await;
    if let Ok(authed) = outcome {
        request.extensions_mut().insert(Tenant(authed.user.id));
        request.extensions_mut().insert(authed);
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use cadus_store::test_support::TestDb;
    use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use sqlx::postgres::PgPoolOptions;
    use sqlx::types::Uuid;
    use tower::ServiceExt;

    use crate::auth::password::Argon2Profile;
    use crate::{AppState, create_app};

    /// A raw session token and the SHA-256 hex that `sha256sum` gives for it.
    ///
    /// The digests below are LITERALS. A fixture that called `hash_token` to
    /// seed the row would agree with a broken `hash_token`; a literal digest
    /// proves that the production digest matches this token.
    const LIVE_TOKEN: (&str, &str) = (
        "fixc-layer-live",
        "b72f8d85131758c102eb9cf0dee6ddee56ad672012a0cf17aaf6fef20800ee2d",
    );

    /// A second raw session token and its digest, for the bearer channel.
    const BEARER_TOKEN: (&str, &str) = (
        "fixc-layer-bearer",
        "6dd23c308b65b87d4dccc3657ff40045cfdb9b1d0a196925663bdbf1c15553b3",
    );

    /// A raw session token whose seeded row is already past `expires_at`.
    const EXPIRED_TOKEN: (&str, &str) = (
        "fixc-layer-expired",
        "75531e854f34326c030d482d3a273e892ceb4e03222ba286a45ebf931f9ecb95",
    );

    /// A raw session token whose account carries a `disabled_at` stamp.
    const DISABLED_TOKEN: (&str, &str) = (
        "fixc-layer-disabled",
        "82ce962a42b8f37e7cc31aa95c592072138f6c9bc04f6073a2c9cf9d3462c9c9",
    );

    /// The session-cookie name of the production posture, written out.
    const COOKIE_NAME: &str = "__Host-cadus_session";

    /// The router under test. It loads NO curriculum, so every route that needs
    /// one answers `503 curriculum_unavailable` once the tenant is bound. That
    /// literal is the proof: an unbound request never reaches the handler.
    fn app(db: &TestDb) -> Router {
        create_app(
            AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
                .with_argon2(Argon2Profile::TEST),
        )
    }

    /// Seed one account and one session row on it, with the given lifetime.
    ///
    /// A negative `expires_in_secs` seeds a row that is already expired.
    async fn seed(db: &TestDb, email: &str, digest: &str, expires_in_secs: i32) -> Uuid {
        let user = db.seed_user(email).await;
        sqlx::query(
            "INSERT INTO auth_sessions (token_hash, user_id, created_at, last_seen_at, expires_at)
             VALUES ($1, $2, now(), now(), now() + make_interval(secs => $3))",
        )
        .bind(digest)
        .bind(user)
        .bind(f64::from(expires_in_secs))
        .execute(&db.admin)
        .await
        .unwrap();
        user
    }

    /// Stamp `disabled_at` on one account.
    async fn disable(db: &TestDb, user: Uuid) {
        sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
    }

    /// The status, the named header, and the parsed body of one call.
    async fn send(app: &Router, request: Request<Body>, header: &str) -> (u16, String, Value) {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status().as_u16();
        let named = response
            .headers()
            .get(header)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, named, body)
    }

    /// The `error.code` of an envelope, or a message that names the miss.
    fn code(body: &Value) -> String {
        match body.get("error").and_then(|error| error.get("code")) {
            Some(Value::String(code)) => code.clone(),
            _ => format!("the body carries no error.code: {body}"),
        }
    }

    /// A `GET` of `path` that presents `token` in the session cookie.
    fn get_with_cookie(path: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(path)
            .header("cookie", format!("{COOKIE_NAME}={token}"))
            .body(Body::empty())
            .unwrap()
    }

    // ------------------------------------------------------------------- //
    // F3: a live session reaches a guarded read, as the RIGHT tenant
    // ------------------------------------------------------------------- //

    /// Review finding F3. `GET /api/export` takes the `Tenant` extractor and
    /// needs no curriculum, so a bound request answers `200` and names the
    /// learner in its `Content-Disposition`. Without the layer the same request
    /// answers `401 unauthorized`.
    ///
    /// The file name carries the account id, so this test proves two things at
    /// once: the layer binds A tenant, and it binds THE tenant of the cookie.
    #[tokio::test]
    async fn a_live_session_cookie_reaches_a_guarded_read_as_that_learner() {
        TestDb::with(|db| async move {
            let app = app(&db);
            let user = seed(&db, "fixc-f3@example.test", LIVE_TOKEN.1, 3600).await;

            let (status, disposition, body) = send(
                &app,
                get_with_cookie("/api/export", LIVE_TOKEN.0),
                "content-disposition",
            )
            .await;

            assert_eq!(status, 200, "the export refused a live session: {body}");
            assert_eq!(
                disposition,
                format!("attachment; filename=\"cadus-export-{user}.jsonl\"")
            );
        })
        .await;
    }

    // ------------------------------------------------------------------- //
    // F4: a live session reaches a guarded WRITE, inside the CSRF layer
    // ------------------------------------------------------------------- //

    /// Review finding F4. `POST /api/enroll` is a guarded write. With the cookie
    /// and a same-origin signal it reaches the handler, which answers
    /// `503 curriculum_unavailable` because this process loaded no curriculum.
    /// Without the layer the same request answers `401 unauthorized`.
    ///
    /// The second call is the layer ORDER: the same write with the cookie and no
    /// origin signal is `403 cross_origin_rejected`. The CSRF layer runs first,
    /// so the ambient cookie is never read as a credential on a forged write.
    #[tokio::test]
    async fn a_live_session_cookie_reaches_a_guarded_write_inside_the_csrf_layer() {
        TestDb::with(|db| async move {
            let app = app(&db);
            seed(&db, "fixc-f4@example.test", LIVE_TOKEN.1, 3600).await;
            let payload = json!({ "course": "c1" }).to_string();

            let same_origin = Request::builder()
                .method("POST")
                .uri("/api/enroll")
                .header("content-type", "application/json")
                .header("cookie", format!("{COOKIE_NAME}={}", LIVE_TOKEN.0))
                .header("sec-fetch-site", "same-origin")
                .body(Body::from(payload.clone()))
                .unwrap();
            let (status, _, body) = send(&app, same_origin, "content-type").await;
            assert_eq!(
                status, 503,
                "the enroll write refused a live session: {body}"
            );
            assert_eq!(code(&body), "curriculum_unavailable");

            let forged = Request::builder()
                .method("POST")
                .uri("/api/enroll")
                .header("content-type", "application/json")
                .header("cookie", format!("{COOKIE_NAME}={}", LIVE_TOKEN.0))
                .body(Body::from(payload))
                .unwrap();
            let (status, _, body) = send(&app, forged, "content-type").await;
            assert_eq!(status, 403, "a forged write was not refused: {body}");
            assert_eq!(code(&body), "cross_origin_rejected");
        })
        .await;
    }

    // ------------------------------------------------------------------- //
    // F7: the layer covers the U7 and U8 task routes, on both channels
    // ------------------------------------------------------------------- //

    /// Review finding F7. `POST /api/task/{task_id}/serve` is a U7 route, and
    /// the bearer header is the second credential channel. A bound request
    /// answers `503 curriculum_unavailable`; without the layer it answers
    /// `401 unauthorized`.
    ///
    /// A bearer write carries no ambient credential, so the CSRF layer lets it
    /// through with no origin header at all (D-M5-5).
    #[tokio::test]
    async fn a_live_bearer_session_reaches_the_task_routes() {
        TestDb::with(|db| async move {
            let app = app(&db);
            seed(&db, "fixc-f7@example.test", BEARER_TOKEN.1, 3600).await;

            let request = Request::builder()
                .method("POST")
                .uri("/api/task/s_2026-01-01a-lesson-addition/serve")
                .header("authorization", format!("Bearer {}", BEARER_TOKEN.0))
                .body(Body::empty())
                .unwrap();
            let (status, _, body) = send(&app, request, "content-type").await;

            assert_eq!(
                status, 503,
                "the serve route refused a live session: {body}"
            );
            assert_eq!(code(&body), "curriculum_unavailable");
        })
        .await;
    }

    // ------------------------------------------------------------------- //
    // F9: the layer covers the U9 diagnosis routes
    // ------------------------------------------------------------------- //

    /// Review finding F9. `GET /api/diagnosis/{id}` is a U9 route. A bound
    /// request for an id that no job row carries answers
    /// `404 unknown_diagnosis`; without the layer it answers
    /// `401 unauthorized`.
    #[tokio::test]
    async fn a_live_session_cookie_reaches_the_diagnosis_poll() {
        TestDb::with(|db| async move {
            let app = app(&db);
            seed(&db, "fixc-f9@example.test", LIVE_TOKEN.1, 3600).await;

            let (status, _, body) = send(
                &app,
                get_with_cookie(
                    "/api/diagnosis/2f1c9d5e-0000-4000-8000-000000000001",
                    LIVE_TOKEN.0,
                ),
                "content-type",
            )
            .await;

            assert_eq!(
                status, 404,
                "the diagnosis poll refused a live session: {body}"
            );
            assert_eq!(code(&body), "unknown_diagnosis");
        })
        .await;
    }

    // ------------------------------------------------------------------- //
    // The closed half: no credential, a bad one, a stale one, a dead account
    // ------------------------------------------------------------------- //

    /// A request with no credential binds nothing, so the extractor answers
    /// `401 unauthorized`. The layer answers nothing of its own.
    #[tokio::test]
    async fn a_request_with_no_credential_is_still_401() {
        TestDb::with(|db| async move {
            let app = app(&db);
            let request = Request::builder()
                .method("GET")
                .uri("/api/export")
                .body(Body::empty())
                .unwrap();
            let (status, _, body) = send(&app, request, "content-type").await;
            assert_eq!(status, 401);
            assert_eq!(code(&body), "unauthorized");
        })
        .await;
    }

    /// A cookie that matches no session row binds nothing.
    #[tokio::test]
    async fn a_cookie_that_matches_no_session_row_is_401() {
        TestDb::with(|db| async move {
            let app = app(&db);
            seed(&db, "fixc-unknown@example.test", LIVE_TOKEN.1, 3600).await;

            let (status, _, body) = send(
                &app,
                get_with_cookie("/api/export", "fixc-layer-no-such-token"),
                "content-type",
            )
            .await;
            assert_eq!(status, 401);
            assert_eq!(code(&body), "unauthorized");
        })
        .await;
    }

    /// A session row that is past `expires_at` binds nothing.
    #[tokio::test]
    async fn an_expired_session_is_401() {
        TestDb::with(|db| async move {
            let app = app(&db);
            seed(&db, "fixc-expired@example.test", EXPIRED_TOKEN.1, -60).await;

            let (status, _, body) = send(
                &app,
                get_with_cookie("/api/export", EXPIRED_TOKEN.0),
                "content-type",
            )
            .await;
            assert_eq!(status, 401);
            assert_eq!(code(&body), "unauthorized");
        })
        .await;
    }

    /// A live session on a disabled account binds nothing.
    #[tokio::test]
    async fn a_live_session_on_a_disabled_account_is_401() {
        TestDb::with(|db| async move {
            let app = app(&db);
            let user = seed(&db, "fixc-disabled@example.test", DISABLED_TOKEN.1, 3600).await;
            disable(&db, user).await;

            let (status, _, body) = send(
                &app,
                get_with_cookie("/api/export", DISABLED_TOKEN.0),
                "content-type",
            )
            .await;
            assert_eq!(status, 401);
            assert_eq!(code(&body), "unauthorized");
        })
        .await;
    }

    /// A store failure binds nothing and answers no `500` of its own.
    ///
    /// The pool below is lazy and points at an address with no server, so the
    /// session lookup fails on every request. The layer must still pass the
    /// request through, and the guarded route must answer its own
    /// `401 unauthorized`. `tests/csrf.rs` asserts the same rule from the other
    /// side, on the same dead pool.
    #[tokio::test]
    async fn a_store_failure_binds_nothing_and_answers_no_500() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
            .unwrap();
        let app = create_app(AppState::new(Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS)));

        let (status, _, body) = send(
            &app,
            get_with_cookie("/api/export", LIVE_TOKEN.0),
            "content-type",
        )
        .await;
        assert_eq!(status, 401);
        assert_eq!(code(&body), "unauthorized");
    }

    /// An unguarded route keeps its own answer under the layer, with a live
    /// credential and without one. The layer answers no request of its own.
    #[tokio::test]
    async fn the_layer_changes_no_unguarded_route() {
        TestDb::with(|db| async move {
            let app = app(&db);
            seed(&db, "fixc-health@example.test", LIVE_TOKEN.1, 3600).await;

            let bare = Request::builder()
                .method("GET")
                .uri("/api/health")
                .body(Body::empty())
                .unwrap();
            let (status, _, _) = send(&app, bare, "content-type").await;
            assert_eq!(status, 200);

            let (status, _, _) = send(
                &app,
                get_with_cookie("/api/health", LIVE_TOKEN.0),
                "content-type",
            )
            .await;
            assert_eq!(status, 200);
        })
        .await;
    }
}
