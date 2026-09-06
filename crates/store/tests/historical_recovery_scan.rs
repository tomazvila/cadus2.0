//! The operator export is tenant-scoped, complete and transactionally read-only.
#![allow(clippy::unwrap_used)]
use std::io::Write;
use std::process::{Command, Stdio};

use cadus_store::test_support::TestDb;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Row};
use uuid::Uuid;

const EXPORT: &str = include_str!("../../../scripts/review/historical_misses_export.sql");

fn scan(rows: &[Value]) -> Value {
    let scripts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/review");
    let mut child = Command::new("python3")
        .args(["-c", "import sys; sys.path.insert(0,sys.argv[1]); import historical_misses as s; print(s.canonical(s.report(sys.stdin)))"])
        .arg(scripts)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for row in rows {
            writeln!(input, "{row}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

async fn seed(db: &TestDb, user: Uuid, email: &str) {
    sqlx::query("INSERT INTO users (id,email) VALUES ($1,$2::text::citext)")
        .bind(user)
        .bind(email)
        .execute(&db.admin)
        .await
        .unwrap();
    let payload = json!({
        "type":"attempt", "v":1, "ts":"2026-01-01T00:00:00Z",
        "attempt_id":"same-id", "task_id":"t1", "topic":"addition",
        "correct":false, "given_answer":"about two",
        "problem":{"text":"Find the value.", "expected":"2"}
    });
    sqlx::query("INSERT INTO events (user_id,seq,ts,type,v,attempt_id,payload) VALUES ($1,1,'2026-01-01T00:00:00Z','attempt',1,'same-id',$2)")
        .bind(user).bind(payload).execute(&db.admin).await.unwrap();
}

#[tokio::test]
async fn historical_export_runs_read_only_and_the_report_contains_only_its_tenant() {
    TestDb::with(|db| async move {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        seed(&db, first, "first@example.test").await;
        seed(&db, second, "second@example.test").await;
        // Replacement is safe because first is a parsed UUID, with no SQL syntax.
        let sql = EXPORT.replace(":'user_id'", &format!("'{first}'"));
        let mut admin = db.admin.acquire().await.unwrap();
        let before: Vec<Value> = sqlx::raw_sql(AssertSqlSafe(sql.clone()))
            .fetch_all(&mut *admin)
            .await
            .unwrap()
            .iter()
            .map(|row| row.get(0))
            .collect();
        assert_eq!(before.len(), 2);
        let report = scan(&before);
        assert_eq!(report["scope"]["event_count"], 1);
        assert_eq!(report["review_required"], 1);
        assert_eq!(report["candidates"][0]["user_id"], first.to_string());
        let mut app = db.app.acquire().await.unwrap();
        let scoped: Vec<Value> = sqlx::raw_sql(AssertSqlSafe(sql.clone()))
            .fetch_all(&mut *app)
            .await
            .unwrap()
            .iter()
            .map(|row| row.get(0))
            .collect();
        assert_eq!(scoped, before);
        assert_eq!(scan(&scoped), report);

        let poisoned = sql.replace(
            "COMMIT;",
            "INSERT INTO users(email) VALUES ('forbidden@example.test'); COMMIT;",
        );
        let error = sqlx::raw_sql(AssertSqlSafe(poisoned))
            .execute(&mut *admin)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("25006")
        );
        sqlx::query("ROLLBACK").execute(&mut *admin).await.unwrap();
        let after: Vec<Value> = sqlx::raw_sql(AssertSqlSafe(sql))
            .fetch_all(&mut *admin)
            .await
            .unwrap()
            .iter()
            .map(|row| row.get(0))
            .collect();
        assert_eq!(after, before);
        let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&mut *admin)
            .await
            .unwrap();
        assert_eq!(users, 2);
    })
    .await;
}
