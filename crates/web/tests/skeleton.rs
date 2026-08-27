//! Proof tests for the rest of M5 U1: the error envelope, the section 3.1
//! security headers, the request-metrics layer, `/metrics`, and the D-M5-6
//! fields of `/api/ready`.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 2 (the envelope),
//! section 3.1 (row "Security headers"), section 10 (row "Ready"), section 11
//! (unit U1: "an unmatched path is one `__unmatched__` metric label"), and
//! ruling D-M5-6 of `docs/plans/M5.md`.
//!
//! Every header value below is a literal of this file, copied from the 1.0
//! source (`cadus_web/app.py:65-89`), never read back from the constant under
//! test. Every metric line is a literal too.
//!
//! Each test builds its own application, so each one gets its own metrics
//! registry and no test can read another test's counts.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::cookie::{
    CookiePosture, CookiePostureError, read_bearer_token, read_session_cookie,
};
use cadus_web::origin::{OriginPolicy, OriginPolicyError};
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

/// The Content-Security-Policy of 1.0, character for character.
const CSP: &str = "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; \
                   font-src 'self'; base-uri 'none'; frame-ancestors 'none'; connect-src 'self' \
                   http://localhost:* http://127.0.0.1:*";

/// The five security headers of spec section 3.1, as literal pairs.
const SECURITY_HEADERS: [(&str, &str); 5] = [
    ("content-security-policy", CSP),
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
    ("referrer-policy", "no-referrer"),
    ("cache-control", "no-cache"),
];

/// An application on a lazy pool that points at an address with no server.
fn offline_app() -> Router {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
        .expect("a lazy pool needs no server");
    app_on(pool)
}

/// An application on `pool`, with the production posture.
fn app_on(pool: PgPool) -> Router {
    create_app(AppState::new(Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS)))
}

/// Send one request and return the status, the headers, and the body as text.
async fn send(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(body.to_vec()).unwrap())
}

/// A `GET` request for `path`.
fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

/// Read `/metrics` and return the exposition text.
async fn scrape(app: &Router) -> String {
    let (status, headers, body) = send(app, get("/metrics")).await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(
        headers.get("content-type").unwrap().to_str().unwrap(),
        "text/plain; version=0.0.4; charset=utf-8"
    );
    body
}

/// Read one header as text.
fn header_of(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .unwrap_or_else(|| panic!("the answer carries no {name} header"))
        .to_str()
        .unwrap()
        .to_string()
}

// ---------------------------------------------------------------------------
// The security headers (spec section 3.1)
// ---------------------------------------------------------------------------

/// (1) Every section 3.1 header literal is on a `200`.
#[tokio::test]
async fn every_security_header_literal_is_on_a_200() {
    let app = offline_app();

    let (status, headers, _body) = send(&app, get("/api/health")).await;

    assert_eq!(status.as_u16(), 200);
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(header_of(&headers, name), value, "header {name}");
    }
}

/// (2) The `403` of the CSRF layer carries the headers too.
///
/// The security-header layer sits OUTSIDE the CSRF layer for exactly this
/// reason. A refusal is still an answer a browser renders.
#[tokio::test]
async fn the_csrf_refusal_carries_the_security_headers() {
    let app = offline_app();

    let request = Request::builder()
        .method("POST")
        .uri("/api/task/t-1/answer")
        .header("host", "tutor.example")
        .header("cookie", "__Host-cadus_session=s3cr3t")
        .header("sec-fetch-site", "cross-site")
        .body(Body::empty())
        .unwrap();
    let (status, headers, _body) = send(&app, request).await;

    assert_eq!(status.as_u16(), 403);
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(header_of(&headers, name), value, "header {name}");
    }
}

/// (3) The `404` fallback and the `405` fallback carry them as well.
#[tokio::test]
async fn both_fallbacks_carry_the_security_headers() {
    let app = offline_app();

    let (not_found, headers, _body) = send(&app, get("/no-such-path")).await;
    assert_eq!(not_found.as_u16(), 404);
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(header_of(&headers, name), value, "404, header {name}");
    }

    let request = Request::builder()
        .method("POST")
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();
    let (not_allowed, headers, _body) = send(&app, request).await;
    assert_eq!(not_allowed.as_u16(), 405);
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(header_of(&headers, name), value, "405, header {name}");
    }
}

/// (4) The layer never overwrites a header the handler set.
///
/// `/api/ready` reports a live reading, so it sets `Cache-Control: no-store`,
/// which is stronger than the layer's `no-cache`. A layer that overwrote it
/// would let a proxy hold a stale readiness answer.
#[tokio::test]
async fn a_handler_keeps_its_own_cache_control() {
    let app = offline_app();

    let (_status, headers, _body) = send(&app, get("/api/ready")).await;

    assert_eq!(header_of(&headers, "cache-control"), "no-store");
    assert_eq!(header_of(&headers, "x-frame-options"), "DENY");
}

// ---------------------------------------------------------------------------
// The error envelope (spec section 2)
// ---------------------------------------------------------------------------

/// (5) An unmatched path answers `404 not_found` inside the envelope.
#[tokio::test]
async fn an_unmatched_path_is_404_not_found_in_the_envelope() {
    let app = offline_app();

    let (status, headers, body) = send(&app, get("/no-such-path")).await;

    assert_eq!(status.as_u16(), 404);
    assert_eq!(header_of(&headers, "content-type"), "application/json");
    assert_eq!(
        body,
        r#"{"error":{"code":"not_found","message":"This path serves nothing."}}"#
    );
}

/// (6) A known path with an unserved method answers `405 method_not_allowed`
/// inside the envelope.
///
/// axum's own fallback answers `405` with an EMPTY body, so a client that
/// branches on `error.code` gets nothing to read. This is the one place that
/// fixes it.
#[tokio::test]
async fn a_wrong_method_is_405_method_not_allowed_in_the_envelope() {
    let app = offline_app();

    let request = Request::builder()
        .method("POST")
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = send(&app, request).await;

    assert_eq!(status.as_u16(), 405);
    assert_eq!(header_of(&headers, "content-type"), "application/json");
    assert_eq!(
        body,
        r#"{"error":{"code":"method_not_allowed","message":"This path does not serve that method."}}"#
    );
}

// ---------------------------------------------------------------------------
// The request-metrics layer and /metrics
// ---------------------------------------------------------------------------

/// (7) The counter is labelled by the matched route TEMPLATE.
#[tokio::test]
async fn the_counter_is_labelled_by_the_route_template() {
    let app = offline_app();

    send(&app, get("/api/health")).await;
    send(&app, get("/api/health")).await;

    let text = scrape(&app).await;

    assert!(
        text.contains(
            "cadus_http_requests_total{method=\"GET\",route=\"/api/health\",status=\"200\"} 2\n"
        ),
        "the scrape carries no /api/health counter:\n{text}"
    );
}

/// (8) U1 acceptance: an unmatched path is ONE `__unmatched__` metric label.
///
/// Three different paths that match no route must fold into one series with the
/// count 3. A label taken from the raw path would give three series here, and a
/// scanner that walks a million paths would then mint a million.
#[tokio::test]
async fn an_unmatched_path_is_one_unmatched_metric_label() {
    let app = offline_app();

    send(&app, get("/nope-one")).await;
    send(&app, get("/nope-two")).await;
    send(&app, get("/deeper/nope/three")).await;

    let text = scrape(&app).await;

    let counter_lines: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("cadus_http_requests_total{"))
        .filter(|line| line.contains("__unmatched__"))
        .collect();

    assert_eq!(
        counter_lines,
        vec!["cadus_http_requests_total{method=\"GET\",route=\"__unmatched__\",status=\"404\"} 3"],
        "three unmatched paths must give exactly one series:\n{text}"
    );

    // No raw path ever reaches a label.
    for path in ["nope-one", "nope-two", "deeper"] {
        assert!(
            !text.contains(path),
            "the raw path {path} reached a metric label:\n{text}"
        );
    }
}

/// (9) The metrics layer counts the `403` of the CSRF layer.
///
/// It sits outside that layer for this reason: a deployment under a CSRF attack
/// must be able to see it in the metrics, and a refusal that no series counts is
/// invisible.
#[tokio::test]
async fn the_csrf_refusal_is_counted() {
    let app = offline_app();

    let request = Request::builder()
        .method("POST")
        .uri("/api/task/t-1/answer")
        .header("host", "tutor.example")
        .header("cookie", "__Host-cadus_session=s3cr3t")
        .header("sec-fetch-site", "cross-site")
        .body(Body::empty())
        .unwrap();
    let (status, _headers, _body) = send(&app, request).await;
    assert_eq!(status.as_u16(), 403);

    let text = scrape(&app).await;

    assert!(
        text.contains(
            "cadus_http_requests_total{method=\"POST\",route=\"__unmatched__\",status=\"403\"} 1\n"
        ),
        "the scrape carries no 403 counter:\n{text}"
    );
}

/// (10) A method outside the known set folds into one `__other__` label.
///
/// HTTP admits any token as a method, so the raw method is an unbounded label
/// and a scanner could mint one series per made-up verb.
#[tokio::test]
async fn an_unknown_method_is_one_other_label() {
    let app = offline_app();

    for verb in ["FROB", "BLORP"] {
        let request = Request::builder()
            .method(verb)
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();
        let (status, _headers, _body) = send(&app, request).await;
        assert_eq!(status.as_u16(), 405);
    }

    let text = scrape(&app).await;

    assert!(
        text.contains(
            "cadus_http_requests_total{method=\"__other__\",route=\"/api/health\",status=\"405\"} 2\n"
        ),
        "two unknown methods must give one series:\n{text}"
    );
    assert!(
        !text.contains("FROB"),
        "the raw method reached a label:\n{text}"
    );
    assert!(
        !text.contains("BLORP"),
        "the raw method reached a label:\n{text}"
    );
}

/// (11) The histogram carries its type line, every bucket bound, and the pair of
/// summary series.
#[tokio::test]
async fn the_duration_histogram_carries_its_buckets_and_summary() {
    let app = offline_app();

    send(&app, get("/api/health")).await;
    let text = scrape(&app).await;

    assert!(text.contains("# TYPE cadus_http_request_duration_seconds histogram\n"));
    for bound in [
        "0.005", "0.01", "0.025", "0.05", "0.1", "0.25", "0.5", "1", "2.5", "5", "10", "+Inf",
    ] {
        let line = format!(
            "cadus_http_request_duration_seconds_bucket{{method=\"GET\",route=\"/api/health\",le=\"{bound}\"}} 1\n"
        );
        assert!(
            text.contains(&line),
            "the scrape has no bucket {bound}:\n{text}"
        );
    }
    assert!(text.contains(
        "cadus_http_request_duration_seconds_count{method=\"GET\",route=\"/api/health\"} 1\n"
    ));
    assert!(
        text.contains(
            "cadus_http_request_duration_seconds_sum{method=\"GET\",route=\"/api/health\"}"
        )
    );
}

/// (12) The histogram drops the status label and folds the statuses of one
/// route together.
///
/// 1.0 labels the latency histogram by method and route only, so a route that
/// answers two different statuses has ONE latency series with both counts in it.
///
/// The two POSTs below are the demonstration. `/api/auth/login` is a real route
/// since M5 U4, so both carry the route TEMPLATE as their label: the first is
/// `403 cross_origin_rejected` from the CSRF layer, the second is
/// `422 invalid_request` from the handler, and the one series counts 2. The two
/// GETs match no route at all, so they fold into the one `__unmatched__` label.
#[tokio::test]
async fn the_histogram_folds_the_statuses_of_one_route() {
    let app = offline_app();

    send(&app, get("/nope-one")).await;
    send(&app, get("/nope-two")).await;

    let refused = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("host", "tutor.example")
        .header("origin", "https://evil.example")
        .body(Body::empty())
        .unwrap();
    let (refused_status, _headers, _body) = send(&app, refused).await;

    let served = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("host", "tutor.example")
        .body(Body::empty())
        .unwrap();
    let (served_status, _headers, _body) = send(&app, served).await;

    let text = scrape(&app).await;

    assert_eq!(refused_status.as_u16(), 403);
    assert_eq!(served_status.as_u16(), 422);
    assert!(text.contains(
        "cadus_http_request_duration_seconds_count{method=\"GET\",route=\"__unmatched__\"} 2\n"
    ));
    assert!(
        text.contains(
            "cadus_http_request_duration_seconds_count{method=\"POST\",route=\"/api/auth/login\"} \
             2\n"
        ),
        "the two statuses of one route must fold into one latency series:\n{text}"
    );
}

/// (13) An empty registry still renders both HELP and TYPE lines.
///
/// A scrape of a process that served nothing must be valid exposition text, not
/// an empty body.
#[tokio::test]
async fn an_empty_registry_renders_both_metric_headers() {
    let app = offline_app();

    let text = scrape(&app).await;

    assert!(text.contains("# TYPE cadus_http_requests_total counter\n"));
    assert!(text.contains("# TYPE cadus_http_request_duration_seconds histogram\n"));
    assert!(
        !text.contains("cadus_http_requests_total{"),
        "the first scrape counts nothing yet:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// /api/ready (D-M5-6)
// ---------------------------------------------------------------------------

/// (14) `/api/ready` reports the age of the oldest unclaimed diagnosis job, and
/// a stale worker is a warning and never a `503`.
///
/// Ruling D-M5-6 takes worker liveness from the `diagnosis_jobs` claim age. Spec
/// section 10, row "Ready": a heartbeat past the threshold is a warning, never a
/// `503`. The learner can still study while the worker is down, because the
/// whole grade verdict is local CPU work (A3) and only the diagnosis prose waits
/// (A4).
///
/// The job row goes in through the ADMIN pool. `diagnosis_jobs` carries a FORCEd
/// tenant policy, and the web pool is unbound, so this test also proves that the
/// SECURITY DEFINER function of migration 0007 is what crosses it: without that
/// function the handler reads zero rows and reports `null` here.
#[tokio::test]
async fn ready_reports_a_stale_worker_as_a_warning_and_not_a_503() {
    TestDb::with(|db| async move {
        let user = db.seed_user("claim-age@example.test").await;
        sqlx::query(
            "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload, created_at) \
             VALUES ($1, 't-1-1', '{}'::jsonb, now() - interval '120 seconds')",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .expect("seed one pending diagnosis job");

        let app = app_on(db.app.clone());
        let (status, _headers, body) = send(&app, get("/api/ready")).await;

        assert_eq!(
            status.as_u16(),
            200,
            "a stale worker is never a 503: {body}"
        );

        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["ok"], serde_json::json!(true));
        assert_eq!(parsed["db"], serde_json::json!("ok"));
        assert_eq!(parsed["worker"]["stale"], serde_json::json!(true));
        assert_eq!(
            parsed["warnings"],
            serde_json::json!(["worker_claim_stale"])
        );

        let age = parsed["worker"]["claim_age_secs"].as_f64().unwrap();
        assert!(
            age >= 120.0,
            "the job was seeded 120 s in the past, the probe read {age}"
        );
    })
    .await;
}

/// (15) A claimed job is no backlog, so the age goes back to `null`.
///
/// The reading counts the jobs that WAIT for a claim. A worker that claims the
/// row is a worker that lives, so a running job must not read as a stale one.
#[tokio::test]
async fn a_claimed_job_leaves_no_backlog() {
    TestDb::with(|db| async move {
        let user = db.seed_user("claimed@example.test").await;
        sqlx::query(
            "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload, created_at, claimed_at, \
             status) VALUES ($1, 't-1-1', '{}'::jsonb, now() - interval '600 seconds', now(), \
             'running')",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .expect("seed one running diagnosis job");

        let app = app_on(db.app.clone());
        let (status, _headers, body) = send(&app, get("/api/ready")).await;

        assert_eq!(status.as_u16(), 200);
        assert_eq!(
            body,
            r#"{"db":"ok","ok":true,"worker":{"claim_age_secs":null,"stale":false}}"#
        );
    })
    .await;
}

/// (16) A young backlog is no warning.
///
/// The threshold is 60 seconds (the 1.0 value). A job that waits a moment is the
/// normal state of a healthy queue.
#[tokio::test]
async fn a_young_backlog_reports_no_warning() {
    TestDb::with(|db| async move {
        let user = db.seed_user("young@example.test").await;
        sqlx::query(
            "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload, created_at) \
             VALUES ($1, 't-1-1', '{}'::jsonb, now() - interval '5 seconds')",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .expect("seed one fresh diagnosis job");

        let app = app_on(db.app.clone());
        let (status, _headers, body) = send(&app, get("/api/ready")).await;

        assert_eq!(status.as_u16(), 200);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["worker"]["stale"], serde_json::json!(false));
        assert_eq!(parsed["warnings"], serde_json::Value::Null);
        let age = parsed["worker"]["claim_age_secs"].as_f64().unwrap();
        assert!((5.0..60.0).contains(&age), "the probe read {age}");
    })
    .await;
}

/// (17) One tenant's backlog is visible to a probe that is bound to nobody, and
/// the answer still names no tenant.
///
/// The readiness probe crosses the tenant policy on purpose (migration 0007).
/// The whole justification is that it reads ONE aggregate number, so the body
/// must never carry an id, an attempt, or a payload.
#[tokio::test]
async fn the_readiness_body_names_no_tenant() {
    TestDb::with(|db| async move {
        let user = db.seed_user("tenant@example.test").await;
        sqlx::query(
            "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload, created_at) \
             VALUES ($1, 't-secret-task-9', '{\"answer\":\"the learner wrote this\"}'::jsonb, \
             now() - interval '90 seconds')",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .expect("seed one pending diagnosis job");

        let app = app_on(db.app.clone());
        let (_status, _headers, body) = send(&app, get("/api/ready")).await;

        assert!(
            !body.contains(&user.to_string()),
            "the body names a user: {body}"
        );
        assert!(
            !body.contains("t-secret-task-9"),
            "the body names an attempt: {body}"
        );
        assert!(
            !body.contains("the learner wrote this"),
            "the body carries a payload: {body}"
        );
    })
    .await;
}

// ---------------------------------------------------------------------------
// The cookie-posture guard and the two configuration readers
// ---------------------------------------------------------------------------

/// (18) The boot guard refuses a `__Host-` cookie without `Secure`.
///
/// A browser discards such a cookie without a word, so the login appears to work
/// and no session ever persists (trap W9). `from_flag` makes the state
/// unreachable today; the guard locks the invariant against a later edit.
#[test]
fn the_cookie_posture_guard_refuses_a_host_cookie_without_secure() {
    let broken = CookiePosture {
        name: "__Host-cadus_session",
        secure: false,
    };

    assert_eq!(
        broken.assert_safe(),
        Err(CookiePostureError::HostPrefixWithoutSecure {
            name: "__Host-cadus_session"
        })
    );
    assert!(
        broken
            .assert_safe()
            .unwrap_err()
            .to_string()
            .contains("needs Secure")
    );

    assert_eq!(CookiePosture::SECURE.assert_safe(), Ok(()));
    assert_eq!(CookiePosture::INSECURE.assert_safe(), Ok(()));
}

/// (19) The one knob expands into the correlated pair, and only `0` and `1` are
/// values.
#[test]
fn the_insecure_cookie_knob_reads_only_zero_and_one() {
    use std::ffi::OsString;

    assert_eq!(CookiePosture::from_env(None), Ok(CookiePosture::SECURE));
    assert_eq!(
        CookiePosture::from_env(Some(OsString::from("0"))),
        Ok(CookiePosture::SECURE)
    );
    assert_eq!(
        CookiePosture::from_env(Some(OsString::from("1"))),
        Ok(CookiePosture::INSECURE)
    );

    assert_eq!(
        CookiePosture::from_env(Some(OsString::from("true"))),
        Err(CookiePostureError::BadFlag {
            value: Some("true".to_string())
        })
    );

    // The pair is correlated, so both halves are pinned together.
    assert_eq!(
        CookiePosture::SECURE,
        CookiePosture {
            name: "__Host-cadus_session",
            secure: true
        }
    );
    assert_eq!(
        CookiePosture::INSECURE,
        CookiePosture {
            name: "cadus_session",
            secure: false
        }
    );
}

/// (20) `PUBLIC_ORIGIN` takes an origin and nothing else.
///
/// A path, a query, or a trailing slash never matches an `Origin` header, so a
/// deployment that set one would refuse every browser write. That is a start
/// error, not a silent fallback.
#[test]
fn public_origin_takes_an_origin_and_nothing_else() {
    use std::ffi::OsString;

    assert_eq!(
        OriginPolicy::from_env(Some(OsString::from("https://tutor.example"))),
        Ok(OriginPolicy {
            public_origin: Some("https://tutor.example".to_string())
        })
    );
    assert_eq!(
        OriginPolicy::from_env(Some(OsString::from("http://127.0.0.1:8080"))),
        Ok(OriginPolicy {
            public_origin: Some("http://127.0.0.1:8080".to_string())
        })
    );
    assert_eq!(
        OriginPolicy::from_env(None),
        Ok(OriginPolicy {
            public_origin: None
        })
    );

    for bad in [
        "https://tutor.example/",
        "https://tutor.example/app",
        "tutor.example",
        "https://",
        "https://tutor.example?x=1",
    ] {
        assert_eq!(
            OriginPolicy::from_env(Some(OsString::from(bad))),
            Err(OriginPolicyError::NotAnOrigin {
                value: bad.to_string()
            }),
            "{bad} is not an origin"
        );
    }
}

/// (21) The bearer reader gives back the token, and gives back nothing for every
/// way the header fails to carry one.
#[test]
fn the_bearer_reader_reads_a_usable_token_only() {
    let with = |value: &str| {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", value.parse().unwrap());
        headers
    };

    assert_eq!(read_bearer_token(&with("Bearer abc123")), Some("abc123"));
    assert_eq!(read_bearer_token(&with("bearer abc123")), Some("abc123"));
    assert_eq!(read_bearer_token(&with("BEARER abc123")), Some("abc123"));
    assert_eq!(
        read_bearer_token(&with("Bearer   abc123  ")),
        Some("abc123")
    );

    assert_eq!(read_bearer_token(&HeaderMap::new()), None);
    assert_eq!(read_bearer_token(&with("Bearer")), None);
    assert_eq!(read_bearer_token(&with("Bearer ")), None);
    assert_eq!(read_bearer_token(&with("Bearer    ")), None);
    assert_eq!(read_bearer_token(&with("Basic dXNlcjpwYXNz")), None);
    assert_eq!(read_bearer_token(&with("Bearerabc123")), None);
}

/// (22) The cookie reader finds one pair among many, and reads the active name
/// only.
#[test]
fn the_cookie_reader_reads_the_named_cookie_only() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "cookie",
        "theme=dark; __Host-cadus_session=s3cr3t; locale=en-US"
            .parse()
            .unwrap(),
    );

    assert_eq!(
        read_session_cookie(&headers, "__Host-cadus_session"),
        Some("s3cr3t")
    );
    assert_eq!(read_session_cookie(&headers, "cadus_session"), None);
    assert_eq!(
        read_session_cookie(&HeaderMap::new(), "cadus_session"),
        None
    );

    // HTTP/2 splits the pairs across several `cookie` headers.
    let mut split = HeaderMap::new();
    split.append("cookie", "theme=dark".parse().unwrap());
    split.append("cookie", "__Host-cadus_session=s3cr3t".parse().unwrap());
    assert_eq!(
        read_session_cookie(&split, "__Host-cadus_session"),
        Some("s3cr3t")
    );
}
