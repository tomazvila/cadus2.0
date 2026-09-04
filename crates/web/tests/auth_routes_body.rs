//! Part of `tests/auth_routes.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;

// ---------------------------------------------------------------------------
// The request-body limit of the write routes (spec section 2, F6)
// ---------------------------------------------------------------------------

/// The seven `/api/auth/*` routes that read a request body.
///
/// The other three routes read none: `logout` and `logout-all` take the session
/// alone, and `me` is a `GET`. No body rejection reaches those three.
const BODY_ROUTES: [&str; 7] = [
    "/api/auth/signup",
    "/api/auth/login",
    "/api/auth/password/change",
    "/api/auth/password/forgot",
    "/api/auth/password/reset",
    "/api/auth/verify-email",
    "/api/auth/verify-email/resend",
];

/// A `POST` of a raw body to `path`, with no credential and no origin header.
///
/// The shared `post` helper renders a `Value`. These two tests send a body that
/// no `Value` can carry: one past the limit, and one that fails mid-read.
fn post_raw(path: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(body)
        .unwrap()
}

/// (31) A body over the limit answers `413 payload_too_large` in the envelope.
///
/// The section 2 envelope has no exception, so the answer carries
/// `{"error":{"code","message"}}` as JSON, not the axum plain-text sentence.
#[tokio::test]
async fn a_body_over_the_limit_is_413_payload_too_large_on_every_write_route() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        // 3 MiB of JSON. It is past the 2 MiB that the server buffers, so the
        // read stops before any handler sees a field.
        let huge = format!(
            "{{\"email\":\"big@example.com\",\"password\":\"{}\"}}",
            "a".repeat(3 * 1024 * 1024)
        );

        for path in BODY_ROUTES {
            let answer = send(&app, post_raw(path, Body::from(huge.clone()))).await;

            assert_eq!(
                answer.status.as_u16(),
                413,
                "route {path} answered {}",
                answer.body
            );
            assert_eq!(answer.code(), "payload_too_large", "route {path}");
            assert_eq!(
                answer
                    .headers
                    .get("content-type")
                    .and_then(|value| value.to_str().ok()),
                Some("application/json"),
                "route {path}"
            );
            assert_eq!(
                answer
                    .body
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str),
                Some("The request body is over the size limit."),
                "route {path}"
            );
        }
    })
    .await;
}

/// (32) A body that the server cannot read answers `422 invalid_request`.
///
/// The stream below fails on its first frame, which is what a client that hangs
/// up mid-body gives. The answer is the envelope, never a plain-text `400`.
#[tokio::test]
async fn a_body_the_server_cannot_buffer_is_422_invalid_request() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let broken = Body::from_stream(tokio_stream::once(Err::<&'static [u8], std::io::Error>(
            std::io::Error::other("the client hung up"),
        )));

        let answer = send(&app, post_raw("/api/auth/signup", broken)).await;

        assert_eq!(answer.status.as_u16(), 422, "body gave {}", answer.body);
        assert_eq!(answer.code(), "invalid_request");
        assert_eq!(
            answer
                .headers
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        assert_eq!(
            answer
                .body
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str),
            Some("The server could not read the request body.")
        );
    })
    .await;
}

// ---------------------------------------------------------------------------
// FIX2-M5-C, finding V5: the email field has a byte cap
// ---------------------------------------------------------------------------

/// How many rate-counter rows this database holds.
///
/// The four public credential routes bump two counters each, so a route that
/// reached its rate rule leaves at least one row behind.
async fn rate_counter_rows(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth_rate_counters")
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// One body of `{"email": <address>}` plus the fields that `extra` names.
fn email_body(address: &str, extra: &[(&str, &str)]) -> Value {
    let mut body = json!({ "email": address });
    for (name, value) in extra {
        body[*name] = json!(value);
    }
    body
}

/// The four public routes that read an email field, with the extra fields each
/// one needs to reach its email step.
const EMAIL_ROUTES: [(&str, &[(&str, &str)]); 4] = [
    ("/api/auth/signup", &[("password", GOOD_PASSWORD)]),
    ("/api/auth/login", &[("password", GOOD_PASSWORD)]),
    ("/api/auth/password/forgot", &[]),
    ("/api/auth/verify-email/resend", &[]),
];

/// (33) A 3 KB address is `422 invalid_request` on all four public routes, and
/// it writes NOTHING.
///
/// Without the cap the normalized address becomes the `key` of the
/// `auth_rate_counters` primary key and the `email` of the `users` unique
/// index. A btree index entry has a hard limit of about 2704 bytes, so a long
/// address of low compressibility makes the rate limiter itself throw, and the
/// very call the limiter must refuse answers `500` uncounted.
///
/// This test pins the CAP, not that index limit: it asserts the `422` and then
/// asserts that both tables stayed empty, so the refusal came before the
/// counter and before the account write. The address here is one repeated
/// character, so Postgres compresses it and the index takes it — which is
/// exactly why the earlier code accepted a 3012-byte address instead of
/// refusing it.
#[tokio::test]
async fn a_three_kilobyte_email_is_422_invalid_request_and_writes_nothing() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let address = format!("{}@example.com", "a".repeat(3000));
        assert_eq!(address.len(), 3012);

        for (path, extra) in EMAIL_ROUTES {
            let answer = send(&app, post(path, &email_body(&address, extra))).await;

            assert_eq!(
                answer.status.as_u16(),
                422,
                "{path} answered {} with {}",
                answer.status,
                answer.body
            );
            assert_eq!(answer.code(), "invalid_request", "{path}");
            assert_eq!(
                answer
                    .body
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str),
                Some("The email address is too long."),
                "{path}"
            );
        }

        assert_eq!(
            rate_counter_rows(&db).await,
            0,
            "an over-cap address must reach no rate counter"
        );
        assert_eq!(
            user_count(&db).await,
            0,
            "an over-cap address must write no account"
        );
    })
    .await;
}

/// (34) The cap is 254 bytes: 254 passes, 255 is refused.
///
/// 254 octets is the longest address that RFC 5321 carries, so the boundary is
/// where a real address stops.
#[tokio::test]
async fn the_email_cap_admits_254_bytes_and_refuses_255() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        // 242 + "@example.com" (12 bytes) = 254 bytes.
        let at_cap = format!("{}@example.com", "a".repeat(242));
        assert_eq!(at_cap.len(), 254);
        // One byte more.
        let over_cap = format!("{}@example.com", "a".repeat(243));
        assert_eq!(over_cap.len(), 255);

        let accepted = send(
            &app,
            post(
                "/api/auth/signup",
                &json!({ "email": at_cap, "password": GOOD_PASSWORD }),
            ),
        )
        .await;
        assert_eq!(accepted.status.as_u16(), 200, "body {}", accepted.body);
        assert_eq!(
            accepted.body.get("status").and_then(Value::as_str),
            Some("verification_required")
        );

        let refused = send(
            &app,
            post(
                "/api/auth/signup",
                &json!({ "email": over_cap, "password": GOOD_PASSWORD }),
            ),
        )
        .await;
        assert_eq!(refused.status.as_u16(), 422, "body {}", refused.body);
        assert_eq!(refused.code(), "invalid_request");

        assert_eq!(
            user_count(&db).await,
            1,
            "only the address at the cap opens an account"
        );
    })
    .await;
}

/// (35) The cap counts the bytes AFTER normalization.
///
/// `U+3316` (`㌖`) is 3 UTF-8 bytes, and NFKC replaces it with the six
/// characters `キロメートル`, which are 18 UTF-8 bytes. The address below is 102
/// bytes on the wire and 552 bytes after normalization, so a cap on the raw
/// field would let it through and a cap on the normalized string refuses it.
/// The normalized string is what reaches the counter key, so the cap counts it.
#[tokio::test]
async fn the_email_cap_counts_the_bytes_after_normalization() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let address = format!("{}@example.com", "\u{3316}".repeat(30));
        assert_eq!(address.len(), 102, "the raw address is under the 254 cap");

        let answer = send(
            &app,
            post(
                "/api/auth/signup",
                &json!({ "email": address, "password": GOOD_PASSWORD }),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 422, "body {}", answer.body);
        assert_eq!(answer.code(), "invalid_request");
        assert_eq!(
            rate_counter_rows(&db).await,
            0,
            "the refusal comes before the rate counter"
        );
    })
    .await;
}

/// (36) A registered over-cap address and an unknown one give the SAME answer.
///
/// The cap runs before every account lookup, so it opens no enumeration
/// channel. The seeded address is 300 bytes, which the `users` unique index
/// still holds, so the known half of the pair really exists.
#[tokio::test]
async fn a_known_and_an_unknown_over_cap_address_answer_the_same() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let known = format!("{}@example.com", "k".repeat(288));
        let unknown = format!("{}@example.com", "u".repeat(288));
        assert_eq!(known.len(), 300);
        assert_eq!(unknown.len(), 300);
        db.seed_user(&known).await;

        for (path, extra) in EMAIL_ROUTES {
            let on_known = send(&app, post(path, &email_body(&known, extra))).await;
            let on_unknown = send(&app, post(path, &email_body(&unknown, extra))).await;

            assert_eq!(on_known.status.as_u16(), 422, "{path}");
            assert_eq!(on_unknown.status.as_u16(), 422, "{path}");
            assert_eq!(on_known.status, on_unknown.status, "{path}");
            assert_eq!(on_known.body, on_unknown.body, "{path}");
        }

        assert_eq!(
            rate_counter_rows(&db).await,
            0,
            "neither half reaches the rate counter"
        );
    })
    .await;
}
