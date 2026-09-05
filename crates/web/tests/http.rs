//! Proof tests for the two probes, the C3 boot guard, and the start sequence of
//! the binary.
//!
//! Tests 1, 2, 3, 6, and 12 drive the router and the guard in process. Tests 4,
//! 5, 7, 8, 9, 10, 13, and 14 start the real binary as a child process, because
//! an exit code and a signal handler exist only in a real process.
//!
//! The `/api/ready` body follows ruling D-M5-6 of `docs/plans/M5.md`: the 1.0
//! `redis` field is gone (2.0 has no Redis, D8) and worker liveness comes from
//! the `diagnosis_jobs` claim age. `crates/web/tests/skeleton.rs` holds the
//! tests of that field; the ones here pin the two `db` verdicts.
//!
//! Every assertion names a literal value: a literal status code, literal body
//! bytes, a literal role name, a literal exit code.
//!
//! Every child process gets `RUST_LOG=info`. An ambient `RUST_LOG` of the
//! developer shell must not decide the result of a test (finding #12).
//!
//! Every test that needs a database uses `TestDb::with`, so a failed assertion
//! drops the throwaway database instead of leaving it on the shared cluster.
//! Every spawned binary sits inside `KillOnDrop`, so a failed assertion also
//! kills and reaps the child instead of leaving a server on a live port
//! (finding #14).
//!
//! Test 10 needs no database: `cadus_store::test_support::DeafPostgres` speaks
//! the Postgres wire protocol itself and stops answering at the exact moment
//! the test wants. Test 11 needs no
//! database either: it proves the `KillOnDrop` guard on an unwind. Test 12
//! points a lazy pool at that same server and measures the readiness probe
//! against the client-side query bound (L1).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;

use std::time::Duration;

use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db, RoleInfo, StoreError};
use cadus_web::{boot_check, create_app};
use http_body_util::BodyExt;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

/// (1) `/api/health` answers 200 with exactly `{"ok":true}`.
///
/// The pool is lazy and points at an address with no server, so the test also
/// proves that liveness does not depend on the database.
#[tokio::test]
async fn health_returns_200_and_exact_body() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
        .expect("a lazy pool needs no server");
    let app = create_app(state_with(pool));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"{\"ok\":true}");
}

/// (2) `/api/ready` answers 200 with `{"ready":true}` on a live app pool.
#[tokio::test]
async fn ready_returns_200_on_a_live_pool() {
    TestDb::with(|db| async move {
        let response = ready_response(db.app.clone()).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            &body[..],
            b"{\"db\":\"ok\",\"ok\":true,\"worker\":{\"claim_age_secs\":null,\"stale\":false}}"
        );
    })
    .await;
}

/// (3) C3: the guard rejects the superuser pool and accepts the app pool.
#[tokio::test]
async fn boot_check_rejects_a_role_that_bypasses_rls() {
    TestDb::with(|db| async move {
        match boot_check(&Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS)).await {
            Err(StoreError::RlsBypass { superuser, .. }) => {
                assert!(superuser, "the test cluster admin is a superuser")
            }
            Err(other) => panic!("the guard gave the wrong error: {other}"),
            Ok(info) => panic!("the guard accepted the superuser role {}", info.name),
        }

        let info = boot_check(&Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .await
            .expect("the guard accepts the app role");
        assert_eq!(
            info,
            RoleInfo {
                name: "cadus_app".to_string(),
                superuser: false,
                bypass_rls: false,
            }
        );
    })
    .await;
}

/// (4) The binary refuses to start with a role that bypasses row-level
/// security. The exit code is exactly 3 and stderr names the reason.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_3_with_a_superuser_dsn() {
    TestDb::with(|db| async move {
        let dsn = dsn_for(&db.name, None);

        let child = spawn_web(
            web_command()
                .env("DATABASE_URL", &dsn)
                .env("BIND_ADDR", "127.0.0.1:0"),
        );

        let (code, stderr) = exit_of(child, Duration::from_secs(10), "boot guard");

        assert_eq!(code, Some(3));
        assert!(
            stderr.contains("bypasses RLS"),
            "stderr does not name the reason: {stderr}"
        );
    })
    .await;
}

/// (5) The binary serves `/api/health` with the app role and stops on SIGTERM
/// with exit code 0, and its log names the port it bound.
///
/// The log line is part of the operator contract: docker-compose.yml and
/// docs/SELF_HOST.md both tell the operator that `cadus-web: listening on`
/// proves the web tier is up, and `web` carries no healthcheck for that reason.
/// The old code put the address in a structured field, so the literal never
/// appeared (finding #10).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_serves_health_and_stops_on_sigterm() {
    TestDb::with(|db| async move {
        let (mut child, address) = web_on_free_port(&db, &[]);

        let (code, body) = wait_until_healthy(child.as_mut(), &address);
        assert_eq!(code, 200);
        assert_eq!(body, "{\"ok\":true}");

        send_sigterm(child.as_ref());

        wait_for_exit(child.as_mut(), Duration::from_secs(5), "graceful shutdown");
        let output = child
            .into_inner()
            .wait_with_output()
            .expect("collect the child output");
        assert_eq!(output.status.code(), Some(0));

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("listening on 127.0.0.1:"),
            "the log must carry the literal `listening on 127.0.0.1:`; stderr:\n{stderr}"
        );
        assert!(
            stderr.contains(&format!("cadus-web: listening on {address}")),
            "the log must name the bound address in the message; stderr:\n{stderr}"
        );
    })
    .await;
}

/// (6) `/api/ready` answers 503 with `{"ready":false}` when the pool is closed.
///
/// A closed pool is the deterministic stand-in for a database that does not
/// answer.
#[tokio::test]
async fn ready_returns_503_on_a_closed_pool() {
    TestDb::with(|db| async move {
        let pool = db.app.clone();
        pool.close().await;
        assert_ready_down(ready_response(pool).await).await;
    })
    .await;
}
