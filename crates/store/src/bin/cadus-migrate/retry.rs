//! The retry of an `ALTER ROLE` statement after "tuple concurrently updated".

use std::future::Future;
use std::time::Duration;

use cadus_store::StoreError;

/// How many times an `ALTER ROLE` statement runs before the program gives up.
const ROLE_ATTEMPT_LIMIT: u32 = 5;

/// The wait between two attempts of an `ALTER ROLE` statement.
const ROLE_RETRY_BACKOFF: Duration = Duration::from_millis(200);

/// The message that PostgreSQL reports when two sessions write one `pg_authid`
/// row at the same time. The condition is transient, so the statement runs
/// again.
const CONCURRENT_UPDATE: &str = "tuple concurrently updated";

/// Run `op` again after a "tuple concurrently updated" error, with the attempt
/// limit and the backoff of this program.
pub async fn retry_concurrent_update<F, Fut>(op: F) -> Result<(), StoreError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), sqlx::Error>>,
{
    retry_with(ROLE_ATTEMPT_LIMIT, ROLE_RETRY_BACKOFF, op).await
}

/// Run `op` again after a "tuple concurrently updated" error.
///
/// `attempts` is the total number of calls of `op`, and `backoff` is the wait
/// between two calls. The loop stops at the first success. An error of another
/// kind stops the loop at the first call, because only this one condition is
/// transient.
///
/// The retry lives here, apart from the statement, so a test drives it with a
/// closure and needs no second writer on the cluster.
async fn retry_with<F, Fut>(attempts: u32, backoff: Duration, mut op: F) -> Result<(), StoreError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), sqlx::Error>>,
{
    let mut attempt: u32 = 1;
    loop {
        let Err(err) = op().await else {
            return Ok(());
        };
        if attempt >= attempts || !is_concurrent_update(&err) {
            return Err(StoreError::Db(err));
        }
        attempt += 1;
        tokio::time::sleep(backoff).await;
    }
}

/// Report whether the error is the transient "tuple concurrently updated"
/// error. An error of another kind stops the run at the first attempt.
fn is_concurrent_update(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db_err) => message_is_concurrent_update(db_err.message()),
        _ => false,
    }
}

/// Report whether a database message names the transient condition.
///
/// PostgreSQL reports "tuple concurrently updated" with SQLSTATE XX000, the
/// code of every internal error, so the message is the one part that names this
/// condition and the code separates nothing.
fn message_is_concurrent_update(message: &str) -> bool {
    message.contains(CONCURRENT_UPDATE)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::cell::Cell;
    use std::error::Error as StdError;
    use std::time::Duration;

    use cadus_store::StoreError;
    use sqlx::error::{DatabaseError, ErrorKind};

    use super::{
        ROLE_ATTEMPT_LIMIT, is_concurrent_update, message_is_concurrent_update, retry_with,
    };

    /// A database error of the test. PostgreSQL reports "tuple concurrently
    /// updated" with SQLSTATE XX000, the code of every internal error, so this
    /// error carries that code and the message decides the retry.
    #[derive(Debug)]
    struct FakeDatabaseError(String);

    impl std::fmt::Display for FakeDatabaseError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }

    impl StdError for FakeDatabaseError {}

    impl DatabaseError for FakeDatabaseError {
        fn message(&self) -> &str {
            &self.0
        }

        fn code(&self) -> Option<Cow<'_, str>> {
            Some(Cow::Borrowed("XX000"))
        }

        fn as_error(&self) -> &(dyn StdError + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn StdError + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn StdError + Send + Sync + 'static> {
            self
        }

        fn kind(&self) -> ErrorKind {
            ErrorKind::Other
        }
    }

    /// Build a database error with this message.
    fn db_error(message: &str) -> sqlx::Error {
        sqlx::Error::Database(Box::new(FakeDatabaseError(message.to_string())))
    }

    /// The Display of a `StoreError::Db` over a database error is the message.
    fn db_message(err: &StoreError) -> String {
        err.to_string()
    }

    /// The message that PostgreSQL reports for the transient condition.
    const CONCURRENT: &str = "tuple concurrently updated";

    /// Run `retry_with` over an operation that fails `failures` times with
    /// the transient message and then succeeds; return the outcome and the
    /// count of calls.
    async fn retried(failures: u32) -> (Result<(), StoreError>, u32) {
        let calls = Cell::new(0u32);
        let outcome = retry_with(ROLE_ATTEMPT_LIMIT, Duration::ZERO, || {
            let call = calls.get() + 1;
            calls.set(call);
            async move {
                if call <= failures {
                    return Err(db_error(CONCURRENT));
                }
                Ok(())
            }
        })
        .await;
        (outcome, calls.get())
    }

    /// Finding "the retry has no end-to-end test": two transient failures and
    /// then a success give `Ok` after exactly three calls.
    #[tokio::test]
    async fn the_retry_stops_at_the_first_success() {
        let (outcome, calls) = retried(2).await;
        assert!(outcome.is_ok(), "expected Ok, got {outcome:?}");
        assert_eq!(calls, 3);
    }

    /// The attempt limit is 5, so six transient failures give an error after
    /// exactly five calls. The error carries the message of the last failure.
    #[tokio::test]
    async fn the_retry_gives_up_at_the_attempt_limit() {
        assert_eq!(ROLE_ATTEMPT_LIMIT, 5);
        let (outcome, calls) = retried(6).await;
        let err = outcome.expect_err("six failures must end in an error");
        assert_eq!(
            db_message(&err),
            "database error: error returned from database: tuple concurrently updated"
        );
        assert_eq!(calls, 5);
    }

    /// An error of another kind is not transient, so the loop stops after
    /// exactly one call and reports that error.
    #[tokio::test]
    async fn another_message_stops_the_retry_at_the_first_call() {
        let calls = Cell::new(0u32);
        let outcome = retry_with(ROLE_ATTEMPT_LIMIT, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            async move { Err(db_error("permission denied for table pg_authid")) }
        })
        .await;

        let err = outcome.expect_err("a permission error must end the run");
        assert_eq!(
            db_message(&err),
            "database error: error returned from database: permission denied for table pg_authid"
        );
        assert_eq!(calls.get(), 1);
    }

    /// Finding #2, second guard: the retry runs for the transient condition
    /// and for no other error, and an error that is not a database error is
    /// never transient.
    #[test]
    fn the_retry_reads_the_concurrent_update_message() {
        assert!(message_is_concurrent_update(
            "tuple concurrently updated at line 1258"
        ));
        assert!(!message_is_concurrent_update(
            "permission denied for table users"
        ));
        assert!(!message_is_concurrent_update(
            "role \"cadus_app\" does not exist"
        ));
        assert!(is_concurrent_update(&db_error(CONCURRENT)));
        assert!(!is_concurrent_update(&sqlx::Error::PoolClosed));
    }

    /// The fake carries the SQLSTATE of an internal error, the `Other` kind,
    /// and its message through every accessor of the trait.
    #[test]
    fn the_fake_database_error_reports_its_code_kind_and_message() {
        let err = db_error("boom");
        let db_err = err.as_database_error().unwrap();
        assert_eq!(db_err.code().as_deref(), Some("XX000"));
        assert_eq!(db_err.kind(), ErrorKind::Other);
        assert_eq!(db_err.as_error().to_string(), "boom");
        let mut boxed: Box<dyn DatabaseError> = Box::new(FakeDatabaseError("boom".to_string()));
        assert_eq!(boxed.as_error_mut().to_string(), "boom");
        assert_eq!(boxed.into_error().to_string(), "boom");
    }
}
