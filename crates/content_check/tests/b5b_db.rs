//! Lane B5b: `--db` against a throwaway database of the test cluster that
//! `CADUS_TEST_DATABASE_URL` names. The database has the table `content_store`
//! with approved and pending rows.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "b5b_support.rs"]
mod support;

use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, ConnectOptions, Connection, PgConnection};
use support::{error_text, fixture, pairs, run};

const TABLE: &str = "CREATE TABLE content_store (digest text PRIMARY KEY, kp_id text NOT NULL, \
kind text NOT NULL, body jsonb NOT NULL, status text NOT NULL, approved_at timestamptz NULL, \
created_at timestamptz NOT NULL DEFAULT now())";

/// The problem of the first exemplar of `precalculus/fx/kp3`.
const KP3_PROBLEM: &str = "Task alpha: compute the alpha-sum for the set three.";

fn cluster() -> PgConnectOptions {
    let dsn = std::env::var("CADUS_TEST_DATABASE_URL").expect(
        "CADUS_TEST_DATABASE_URL is not set; example: postgresql://test:test@127.0.0.1:55436/postgres",
    );
    dsn.parse()
        .expect("CADUS_TEST_DATABASE_URL is a Postgres DSN")
}

async fn seed(options: &PgConnectOptions) {
    let pool = PgPoolOptions::new()
        .connect_with(options.clone())
        .await
        .unwrap();
    sqlx::query(TABLE).execute(&pool).await.unwrap();
    let teach = |problem: &str| json!({"concept": "c", "worked_example": {"problem": problem, "steps": ["s"]}});
    let rows: [(&str, &str, &str, Value, &str); 5] = [
        (
            "d1",
            "fx/kp1",
            "teach",
            teach("A problem of the teach page."),
            "approved",
        ),
        ("d2", "fx/kp1", "template", json!({"t": 1}), "approved"),
        ("d3", "fx/kp3", "teach", teach(KP3_PROBLEM), "approved"),
        // Rows that are not approved do not count.
        ("d4", "fx/kp2", "teach", teach("A pending page."), "pending"),
        ("d5", "fx/kp2", "template", json!({"t": 2}), "rejected"),
    ];
    for (digest, kp, kind, body, status) in rows {
        sqlx::query("INSERT INTO content_store (digest, kp_id, kind, body, status) VALUES ($1, $2, $3, $4, $5)")
            .bind(digest)
            .bind(kp)
            .bind(kind)
            .bind(body)
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;
}

/// Run `body` with the DSN of a fresh seeded database, then drop the database.
fn with_database(body: impl FnOnce(&str) + std::panic::UnwindSafe) {
    with_fresh_database(true, body);
}

/// Run `body` with the DSN of a fresh database, seeded with `content_store` when
/// `seeded` is true and empty otherwise, then drop the database. Each call gets
/// its own name, so tests that run in parallel never share a database.
fn with_fresh_database(seeded: bool, body: impl FnOnce(&str) + std::panic::UnwindSafe) {
    let name = format!(
        "cc_b5b_{}_{}",
        std::process::id(),
        if seeded { "seeded" } else { "empty" }
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let admin = |statement: String| async move {
        let mut connection = PgConnection::connect_with(&cluster()).await.unwrap();
        sqlx::query(AssertSqlSafe(statement))
            .execute(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
    };
    runtime.block_on(admin(format!("CREATE DATABASE {name}")));
    let options = cluster().database(&name);
    if seeded {
        runtime.block_on(seed(&options));
    }
    let dsn = options.to_url_lossy().to_string();
    let outcome = std::panic::catch_unwind(|| body(&dsn));
    runtime.block_on(admin(format!("DROP DATABASE {name} WITH (FORCE)")));
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

fn kp_of<'a>(doc: &'a Value, kp: &str) -> &'a Value {
    let kps = doc["kps"].as_array().unwrap();
    kps.iter().find(|entry| entry["kp"] == kp).unwrap()
}

fn rules(kp: &Value) -> Vec<String> {
    pairs(&kp["findings"])
        .into_iter()
        .map(|pair| pair.1)
        .collect()
}

#[test]
fn the_db_option_reads_the_approved_teach_pages_and_templates() {
    with_database(|dsn| {
        let tree = fixture("tree");
        let result = run(&[
            "report",
            "--course",
            "precalculus",
            "--base",
            &tree,
            "--db",
            dsn,
        ]);
        assert_eq!(result.exit, 1);
        let doc = &result.doc;
        assert_eq!(doc["db_read"], true);
        // kp1: an approved teach page and one approved template; no finding.
        let kp1 = kp_of(doc, "precalculus/fx/kp1");
        assert_eq!((&kp1["teach"], &kp1["T"]), (&json!(true), &json!(1)));
        assert_eq!(kp1["findings"], json!([]));
        // kp2: a pending page and a rejected template only: I15, T = 0.
        let kp2 = kp_of(doc, "precalculus/fx/kp2");
        assert_eq!((&kp2["teach"], &kp2["T"]), (&json!(false), &json!(0)));
        assert_eq!(rules(kp2), ["I2", "I9", "I15"]);
        assert_eq!(kp2["findings"][2]["code"], "invariant:I15");
        // kp3: the worked example of the teach page is equal to exemplar 1.
        let kp3 = kp_of(doc, "precalculus/fx/kp3");
        assert_eq!(rules(kp3), ["I2", "I5"]);
        assert_eq!(kp3["existing"][0]["status"], "must_replace");
        // 26 KPs of the course, 2 with a teach page.
        assert_eq!(doc["courses"][0]["kps_no_teach_page"], 24);
        assert_eq!(doc["courses"][0]["breaches"]["I15"], 24);

        let dump = run(&[
            "dump-kp",
            "--kp",
            "precalculus/fx/kp3",
            "--base",
            &tree,
            "--db",
            dsn,
        ]);
        assert_eq!(dump.doc["exemplars"][0]["status"], "must_replace");
        let reason = dump.doc["exemplars"][0]["reason"].as_str().unwrap();
        assert!(reason.contains("teach page"), "{reason}");

        let row = json!({"kp": "precalculus/fx/kp1", "items": [{"id": "n1", "rule": "R1",
            "problem": "A problem of the teach page.", "answer": "5",
            "answer_contract": {"kind": "exact"}, "solution_sketch": "s"}]});
        let path = support::scratch("b5b_db.row.json", &row.to_string());
        let check = run(&["row", "--row", &path, "--base", &tree, "--db", dsn]);
        let found = pairs(&check.doc["findings"]);
        assert!(
            found.contains(&("precalculus/fx/kp1".to_owned(), "I5".to_owned())),
            "{}",
            check.doc
        );
    });
}

#[test]
fn a_database_that_has_no_content_store_is_exit_2() {
    // A fresh empty database: the database that `CADUS_TEST_DATABASE_URL` names
    // may carry the migrated schema, `content_store` included.
    with_fresh_database(false, |dsn| {
        let text = error_text(
            &run(&["report", "--all", "--base", &fixture("tree"), "--db", dsn]),
            2,
        );
        assert!(
            text.starts_with("--db: cannot read content_store"),
            "{text}"
        );
    });
    let closed = "postgresql://test:test@127.0.0.1:1/postgres";
    let text = error_text(
        &run(&[
            "report",
            "--all",
            "--base",
            &fixture("tree"),
            "--db",
            closed,
        ]),
        2,
    );
    assert!(text.starts_with("--db: cannot connect"), "{text}");
}
