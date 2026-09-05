//! The state helpers of the fold tests: a locked tenant transaction, a batch
//! append, and the three reads that a test takes outside its transaction.

use cadus_core::event::Event;

use super::events::{SESSION, attempt, start};
use cadus_store::begin_tenant;
use cadus_store::state::{
    CachedModel, EventRow, SessionView, append_event, load_events, load_learner_model,
    load_session_view, lock_web_state,
};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use cadus_store::Db;
use cadus_store::test_support::TestDb;

use super::app_db;
use super::events::Fixture;

/// A tenant transaction of `user` on `handle` that holds the `web_state` lock.
pub async fn open_locked(handle: &Db, user: Uuid) -> Transaction<'static, Postgres> {
    let mut tx = begin_tenant(handle.pool(), user)
        .await
        .expect("the tenant transaction starts");
    lock_web_state(&mut tx, user)
        .await
        .expect("the lock is taken");
    tx
}

/// Append every event of `events`, with its attempt id, in order.
pub async fn append_all(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    events: &[(&Event, Option<&str>)],
) {
    for (event, attempt_id) in events {
        append_event(tx, user, event, *attempt_id)
            .await
            .expect("the event appends");
    }
}

/// The whole log of `user`, read in a transaction that is rolled back.
pub async fn read_log(handle: &Db, user: Uuid) -> Vec<EventRow> {
    let mut tx = begin_tenant(handle.pool(), user)
        .await
        .expect("the tenant transaction starts");
    let rows = load_events(&mut tx, user).await.expect("the log reads");
    tx.rollback().await.expect("the transaction rolls back");
    rows
}

/// The cached model of `user`, read in a transaction that is rolled back.
pub async fn read_cache(handle: &Db, user: Uuid) -> CachedModel {
    let mut tx = begin_tenant(handle.pool(), user)
        .await
        .expect("the tenant transaction starts");
    let cached = load_learner_model(&mut tx, user)
        .await
        .expect("the cache reads")
        .expect("the learner has a cache row");
    tx.rollback().await.expect("the transaction rolls back");
    cached
}

/// The session view of `user`, read in a transaction that is rolled back.
pub async fn read_view(handle: &Db, user: Uuid) -> SessionView {
    let mut tx = begin_tenant(handle.pool(), user)
        .await
        .expect("the tenant transaction starts");
    let view = load_session_view(&mut tx, user)
        .await
        .expect("the view reads");
    tx.rollback().await.expect("the transaction rolls back");
    view
}

/// A locked tenant transaction of `user` with `events` already appended.
pub async fn open_with(
    handle: &Db,
    user: Uuid,
    events: &[(&Event, Option<&str>)],
) -> Transaction<'static, Postgres> {
    let mut tx = open_locked(handle, user).await;
    append_all(&mut tx, user, events).await;
    tx
}

/// A locked tenant transaction of `user` whose log holds the session start
/// and the first attempt `t-1`.
pub async fn open_first_session(handle: &Db, user: Uuid) -> Transaction<'static, Postgres> {
    open_with(
        handle,
        user,
        &[(&start(SESSION), None), (&attempt("t-1"), Some("t-1"))],
    )
    .await
}

/// The app handle and the micro fixture of one fold test.
pub struct Scene {
    pub handle: Db,
    pub fixture: Fixture,
}

impl Scene {
    /// The app pool of `db` with the shipped bound, and the micro fixture.
    pub fn new(db: &TestDb) -> Self {
        Self {
            handle: app_db(db),
            fixture: Fixture::micro(),
        }
    }

    /// The projection input of the fixture.
    pub fn input(&self) -> cadus_core::projector::ProjectionInput<'_> {
        self.fixture.input()
    }
}
