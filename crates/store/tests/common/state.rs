//! The state helpers of the fold tests: a locked tenant transaction, a batch
//! append, and the three reads that a test takes outside its transaction.

use cadus_core::event::Event;
use cadus_store::begin_tenant;
use cadus_store::state::{
    CachedModel, EventRow, SessionView, append_event, load_events, load_learner_model,
    load_session_view, lock_web_state,
};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use cadus_store::Db;

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
