//! `--db`: the approved teach pages and templates of `content_store`.
//!
//! The session is read-only (`default_transaction_read_only`), and the one
//! statement is a SELECT. A database error is an input error (exit 2).

use std::collections::BTreeMap;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use super::Fail;

/// The newest approved row comes first for each KP.
const QUERY: &str = "SELECT kp_id, kind, body #>> '{worked_example,problem}' AS problem \
FROM content_store WHERE status = 'approved' AND kind IN ('teach', 'template') \
ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest";

/// One row of the query: `kp_id`, `kind`, the worked-example problem.
type StoreRow = (String, String, Option<String>);

/// The approved content of each KP. The key is `topic/kp`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Store {
    /// The worked-example problem of the approved teach page.
    teach: BTreeMap<String, Option<String>>,
    /// The count of approved templates.
    templates: BTreeMap<String, usize>,
}

impl Store {
    /// Read the store through a read-only session.
    pub fn read(dsn: &str) -> Result<Self, Fail> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        fail("cannot start the tokio runtime", runtime)
            .and_then(|runtime| runtime.block_on(fetch(dsn)))
    }

    /// True if the KP has an approved teach page (I15).
    pub fn has_teach(&self, key: &str) -> bool {
        self.teach.contains_key(key)
    }

    /// The worked-example problem of the approved teach page (I5).
    pub fn teach_problem(&self, key: &str) -> Option<&str> {
        self.teach.get(key).and_then(Option::as_deref)
    }

    /// T: the count of approved templates of the KP.
    pub fn templates(&self, key: &str) -> usize {
        self.templates.get(key).copied().unwrap_or(0)
    }

    fn add(&mut self, (key, kind, problem): StoreRow) {
        if kind == "teach" {
            self.teach.entry(key).or_insert(problem);
        } else {
            *self.templates.entry(key).or_insert(0) += 1;
        }
    }
}

fn fail<T, E: std::fmt::Display>(what: &str, result: Result<T, E>) -> Result<T, Fail> {
    result.map_err(|error| Fail::input(format!("--db: {what}: {error}")))
}

async fn fetch(dsn: &str) -> Result<Store, Fail> {
    let options = fail("cannot parse the DSN", dsn.parse::<PgConnectOptions>())?
        .options([("default_transaction_read_only", "on")]);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect_with(options)
        .await;
    let pool = fail("cannot connect", pool)?;
    let rows = sqlx::query_as::<_, StoreRow>(QUERY).fetch_all(&pool).await;
    pool.close().await;
    let mut store = Store::default();
    for row in fail("cannot read content_store", rows)? {
        store.add(row);
    }
    Ok(store)
}
