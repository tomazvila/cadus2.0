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

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::net::TcpListener;
use std::os::unix::ffi::OsStrExt;
use std::thread::sleep;
use std::time::{Duration, Instant};

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

/// A start with every setting valid and a database that refuses the
/// connection ends with exit code 2.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_when_the_database_refuses_the_connection() {
    let child = spawn_web(
        web_command()
            .env("DATABASE_URL", UNREACHED_DSN)
            .env("BIND_ADDR", "127.0.0.1:0"),
    );

    let (code, stderr) = exit_of(child, Duration::from_secs(10), "connection refused");

    assert_eq!(code, Some(2), "stderr:\n{stderr}");
}

/// A stop signal during the connect gives exit code 0. The DSN points at a
/// socket that accepts the connection and answers nothing, so the connect
/// never returns on its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_zero_on_sigterm_during_the_connect() {
    let silent = TcpListener::bind("127.0.0.1:0").expect("bind the silent socket");
    silent
        .set_nonblocking(true)
        .expect("make the silent socket nonblocking");
    let port = silent.local_addr().expect("the silent address").port();
    let mut child = spawn_web(
        web_command()
            .env("DATABASE_URL", format!("postgresql://x@127.0.0.1:{port}/x"))
            .env("BIND_ADDR", "127.0.0.1:0"),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let held_connection = loop {
        match silent.accept() {
            Ok((connection, _peer)) => break connection,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if let Some(status) = child.as_mut().try_wait().expect("poll cadus-web") {
                    panic!("cadus-web exited before the database connect: {status:?}");
                }
                if Instant::now() >= deadline {
                    panic!("cadus-web did not start the database connect within 10 s");
                }
                sleep(Duration::from_millis(50));
            }
            Err(error) => panic!("accept the database connection: {error}"),
        }
    };
    send_sigterm(child.as_ref());

    let (code, stderr) = exit_of(child, Duration::from_secs(10), "stop during connect");

    assert_eq!(code, Some(0), "stderr:\n{stderr}");
    assert!(
        stderr.contains("the stop signal came before the database connect"),
        "stderr does not name the early stop: {stderr}"
    );
    drop(held_connection);
    drop(silent);
}

/// A bind address another socket holds ends the start with exit code 2, after
/// the boot guard passed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_when_the_bind_address_is_in_use() {
    TestDb::with(|db| async move {
        let holder = TcpListener::bind("127.0.0.1:0").expect("bind the holder");
        let address = holder.local_addr().expect("the held address").to_string();
        let child = spawn_web(
            web_command()
                .env("DATABASE_URL", dsn_for(&db.name, Some("cadus_app")))
                .env("BIND_ADDR", &address),
        );

        let (code, stderr) = exit_of(child, Duration::from_secs(20), "bind in use");

        assert_eq!(code, Some(2), "stderr:\n{stderr}");
        assert!(
            stderr.contains(&format!("bind {address} failed")),
            "stderr does not name the bind: {stderr}"
        );
        drop(holder);
    })
    .await;
}

/// An admin DSN whose database refuses the connection ends the start with
/// exit code 2, and the message names the variable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_with_an_admin_dsn_that_does_not_connect() {
    TestDb::with(|db| async move {
        let (child, _address) =
            web_on_free_port(&db, &[("CADUS_ADMIN_DATABASE_URL", UNREACHED_DSN)]);

        let (code, stderr) = exit_of(child, Duration::from_secs(20), "admin connect");

        assert_eq!(code, Some(2), "stderr:\n{stderr}");
        assert!(
            stderr.contains("CADUS_ADMIN_DATABASE_URL"),
            "stderr does not name the variable: {stderr}"
        );
    })
    .await;
}

/// A shutdown deadline that is not valid Unicode is a start error that names
/// it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_with_a_deadline_that_is_not_unicode() {
    let child = spawn_web(
        web_command()
            .env("DATABASE_URL", UNREACHED_DSN)
            .env("BIND_ADDR", "127.0.0.1:0")
            .env("SHUTDOWN_DEADLINE_SECS", OsStr::from_bytes(b"7\xff")),
    );

    let (code, stderr) = exit_of(child, Duration::from_secs(10), "deadline");

    assert_eq!(code, Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("SHUTDOWN_DEADLINE_SECS is not valid Unicode"),
        "stderr does not name the variable: {stderr}"
    );
}

/// An admin DSN that is not valid Unicode is a start error that names it. The
/// admin DSN is read after the boot guard, so the tenant database is live.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_with_an_admin_dsn_that_is_not_unicode() {
    TestDb::with(|db| async move {
        let child = spawn_web(
            web_command()
                .env("DATABASE_URL", dsn_for(&db.name, Some("cadus_app")))
                .env("BIND_ADDR", "127.0.0.1:0")
                .env("CADUS_ADMIN_DATABASE_URL", OsStr::from_bytes(b"x\xff")),
        );

        let (code, stderr) = exit_of(child, Duration::from_secs(20), "admin dsn");

        assert_eq!(code, Some(2), "stderr:\n{stderr}");
        assert!(
            stderr.contains("CADUS_ADMIN_DATABASE_URL is not valid Unicode"),
            "stderr does not name the variable: {stderr}"
        );
    })
    .await;
}

/// Without `RUST_LOG` the filter is `info`, and without `CADUS_CURRICULUM`
/// the default path is read: neither absence stops the start on its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_reads_the_default_log_filter_and_curriculum_path() {
    let child = spawn_web(
        web_command()
            .env("DATABASE_URL", UNREACHED_DSN)
            .env("BIND_ADDR", "127.0.0.1:0")
            .env_remove("RUST_LOG")
            .env_remove("CADUS_CURRICULUM"),
    );

    let (code, stderr) = exit_of(child, Duration::from_secs(10), "defaults");

    // The default tree is not under the test directory, or the database
    // refuses the connection: both end the start with exit code 2 after the
    // defaults were read.
    assert_eq!(code, Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cadus-web"),
        "the default filter printed no line: {stderr}"
    );
}

/// `SIGINT` stops a healthy server the same way `SIGTERM` does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_zero_on_sigint() {
    TestDb::with(|db| async move {
        let (mut child, address) = web_on_free_port(&db, &[]);
        let (code, _body) = wait_until_healthy(child.as_mut(), &address);
        assert_eq!(code, 200);

        send_signal(child.as_ref(), "-INT");
        let (code, stderr) = exit_of(child, Duration::from_secs(10), "SIGINT stop");

        assert_eq!(code, Some(0), "stderr:\n{stderr}");
        assert!(
            stderr.contains("SIGINT received"),
            "stderr does not name the signal: {stderr}"
        );
    })
    .await;
}

/// A start with a valid PUBLIC_ORIGIN skips the origin-fallback note, so the
/// origin policy pins the named origin. A healthy server then stops on
/// SIGTERM with exit code 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_pins_a_public_origin_and_stops_clean() {
    TestDb::with(|db| async move {
        let (mut child, address) =
            web_on_free_port(&db, &[("PUBLIC_ORIGIN", "https://tutor.example")]);
        let (code, _body) = wait_until_healthy(child.as_mut(), &address);
        assert_eq!(code, 200);

        send_sigterm(child.as_ref());
        let (code, stderr) = exit_of(child, Duration::from_secs(10), "public-origin stop");
        assert_eq!(code, Some(0), "stderr:\n{stderr}");
        assert!(
            !stderr.contains("PUBLIC_ORIGIN is not set"),
            "the origin-fallback note must not print when PUBLIC_ORIGIN is set: {stderr}"
        );
    })
    .await;
}
