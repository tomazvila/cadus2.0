//! The start sequence of the `cadus-web` binary on its warn paths, its
//! remaining start errors, and the admin connection.
//!
//! Every test here starts the real binary as a child process, because a warn
//! line, an exit code, and a second pool exist only in a real process. The
//! child writes its own coverage profile, so these paths count.
//!
//! `tests/http.rs` and `tests/http_binary.rs` cover the probes, the boot guard,
//! the bad `BIND_ADDR`, the bad cookie posture, and the bounded shutdown. This
//! file covers the four values the start reads AFTER the pool config
//! (`SHUTDOWN_DEADLINE_SECS`, `CADUS_AUTH_ARGON2_PROFILE`, the OAuth
//! credentials, and `PUBLIC_ORIGIN`), the curriculum that does not load, and
//! the admin pool of `CADUS_ADMIN_DATABASE_URL`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;

use std::time::Duration;

use cadus_store::test_support::TestDb;

/// A DSN this start never connects: the start stops before the connect.
const UNREACHED_DSN: &str = "postgresql://x@127.0.0.1:1/x";

/// A `SHUTDOWN_DEADLINE_SECS` that is not a whole number is a start error.
///
/// The value parses AFTER the pool config, so the unreachable DSN below costs
/// the test no time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_with_a_bad_shutdown_deadline() {
    let child = spawn_web(
        web_command()
            .env("DATABASE_URL", UNREACHED_DSN)
            .env("BIND_ADDR", "127.0.0.1:0")
            .env("SHUTDOWN_DEADLINE_SECS", "many"),
    );

    let (code, stderr) = exit_of(child, Duration::from_secs(10), "bad shutdown deadline");

    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("SHUTDOWN_DEADLINE_SECS"),
        "stderr does not name the variable: {stderr}"
    );
}

/// A `CADUS_AUTH_ARGON2_PROFILE` that names no profile is a start error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_with_an_unknown_argon2_profile() {
    let child = spawn_web(
        web_command()
            .env("DATABASE_URL", UNREACHED_DSN)
            .env("BIND_ADDR", "127.0.0.1:0")
            .env("CADUS_AUTH_ARGON2_PROFILE", "prd"),
    );

    let (code, stderr) = exit_of(child, Duration::from_secs(10), "bad argon2 profile");

    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("CADUS_AUTH_ARGON2_PROFILE"),
        "stderr does not name the variable: {stderr}"
    );
}

/// A curriculum that does not load is a start error, and the warn lines of the
/// dev cookie, the test Argon2 profile, the set OAuth credentials, and the
/// absent `PUBLIC_ORIGIN` all print before it.
///
/// Every value here reads AFTER the pool config and BEFORE the curriculum, so
/// one process drives all four warn paths and the curriculum-error path
/// together. The DSN is never connected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_with_a_curriculum_that_does_not_load() {
    let child = spawn_web(
        web_command()
            .env("DATABASE_URL", UNREACHED_DSN)
            .env("BIND_ADDR", "127.0.0.1:0")
            .env("CADUS_WEB_INSECURE_COOKIE", "1")
            .env("CADUS_AUTH_ARGON2_PROFILE", "test")
            .env("OAUTH_GOOGLE_CLIENT_ID", "start-google-id")
            .env("OAUTH_GOOGLE_CLIENT_SECRET", "start-google-secret")
            .env_remove("PUBLIC_ORIGIN")
            .env("CADUS_CURRICULUM", "/no/such/curriculum/tree"),
    );

    let (code, stderr) = exit_of(child, Duration::from_secs(10), "curriculum load");

    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("the curriculum at /no/such/curriculum/tree did not load"),
        "stderr does not name the curriculum failure: {stderr}"
    );
    assert!(
        stderr.contains("CADUS_WEB_INSECURE_COOKIE=1"),
        "the dev-cookie warn did not print: {stderr}"
    );
    assert!(
        stderr.contains("OAuth credentials are set"),
        "the OAuth warn did not print: {stderr}"
    );
    assert!(
        stderr.contains("PUBLIC_ORIGIN is not set"),
        "the origin fallback note did not print: {stderr}"
    );
}

/// The binary opens the admin pool when `CADUS_ADMIN_DATABASE_URL` names one,
/// serves `/api/health`, and stops on SIGTERM with exit code 0.
///
/// The admin DSN is the superuser pool of the throwaway database, which is the
/// production `cadus_admin` role in every way this start reads: it connects and
/// it needs no C3 boot guard.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_opens_the_admin_pool_when_the_admin_dsn_is_set() {
    TestDb::with(|db| async move {
        let admin_dsn = dsn_for(&db.name, None);
        let (mut child, address) =
            web_on_free_port(&db, &[("CADUS_ADMIN_DATABASE_URL", admin_dsn.as_str())]);

        let (code, _body) = wait_until_healthy(child.as_mut(), &address);
        assert_eq!(code, 200);

        send_sigterm(child.as_ref());
        wait_for_exit(child.as_mut(), Duration::from_secs(10), "admin-pool stop");
        let (code, stderr) = collect_exit(child);
        assert_eq!(
            code,
            Some(0),
            "the server with an admin pool must exit 0; stderr:\n{stderr}"
        );
    })
    .await;
}
