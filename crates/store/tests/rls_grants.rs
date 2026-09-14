//! Proof tests for the database-level guarantees of M0, part 1: the grant
//! surface of the two roles on the tables that carry no tenant policy (C2,
//! D9).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::begin_tenant;
use cadus_store::test_support::TestDb;
use common::sqlstate;

/// C2: the app role appends to `events` and never edits or erases a row.
#[tokio::test]
async fn app_role_cannot_update_events() {
    TestDb::with(|db| async move {
        let user = db.seed_user("append-only@example.test").await;

        sqlx::query!(
            "INSERT INTO events (user_id, seq, ts, type, payload)
             VALUES ($1, 1, now(), 'attempt', '{}'::jsonb)",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let update_err = sqlx::query!("UPDATE events SET type = 'x'")
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&update_err), "42501");
        let _ = tx.rollback().await;

        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let delete_err = sqlx::query!("DELETE FROM events")
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&delete_err), "42501");
        let _ = tx.rollback().await;

        // Append-only, not read-only: the insert of a second event succeeds.
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        sqlx::query!(
            "INSERT INTO events (user_id, seq, ts, type, payload)
             VALUES ($1, 2, now(), 'attempt', '{}'::jsonb)",
            user
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(count, 2);
        let _ = tx.rollback().await;
    })
    .await;
}

/// C3, finding #2: the app role cannot delete a `users` row.
///
/// `users` stays outside the `tenant_isolation` set, because its key is `id`,
/// and every tenant table points at it with `ON DELETE CASCADE`. Postgres runs a
/// referential-action trigger with row-level security off, so a `DELETE` on
/// `users` erases another tenant's rows through the cascade. Account deletion is
/// an admin operation. Findings #4 and #11 narrowed SELECT, INSERT, and UPDATE;
/// `app_role_reads_only_its_own_user_row`,
/// `app_role_inserts_no_id_and_no_admin_flag`, and
/// `app_role_updates_only_its_own_user_row` prove those parts.
#[tokio::test]
async fn app_role_cannot_delete_users() {
    TestDb::with(|db| async move {
        let user = db.seed_user("cascade-guard@example.test").await;

        sqlx::query!(
            "INSERT INTO learner_models
                 (user_id, model, through_seq, projector_version, config_hash)
             VALUES ($1, '{}'::jsonb, 0, 1, 'test')",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let delete_err = sqlx::query!("DELETE FROM users")
            .execute(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&delete_err), "42501");

        let truncate_err = sqlx::query("TRUNCATE users CASCADE")
            .execute(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&truncate_err), "42501");

        // The cascade never ran: the child row of the tenant is still there.
        let children = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM learner_models"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(children, 1);

        // #11: an unbound SELECT of the runtime role reads no row of users.
        let seen = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM users"#)
            .fetch_one(&db.app)
            .await
            .unwrap();
        assert_eq!(seen, 0);

        // #4: an UPDATE of the caller's own row still succeeds inside a tenant.
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let updated = sqlx::query!(
            "UPDATE users SET email_verified_at = now() WHERE id = $1",
            user
        )
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(updated, 1);
        tx.commit().await.unwrap();
    })
    .await;
}

/// D9, finding #8: the app role holds no privilege on the migration ledger.
///
/// sqlx creates `_sqlx_migrations` before the first migration runs, so the
/// blanket grant in 0006 swept it in. A `DELETE` on the ledger makes the next
/// deploy replay 0002 and stop with an error.
#[tokio::test]
async fn app_role_cannot_touch_the_migration_ledger() {
    TestDb::with(|db| async move {
        let read_err = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM _sqlx_migrations"#)
            .fetch_one(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&read_err), "42501");

        let delete_err = sqlx::query!("DELETE FROM _sqlx_migrations")
            .execute(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&delete_err), "42501");
    })
    .await;
}

/// T6, findings #24 and #5: `cadus_admin` writes `model_call_log`, and the
/// runtime role reaches it with no statement at all.
///
/// Finding #24: `model_call_log.id` is the only bigserial column of the schema,
/// so the insert needs `USAGE` on `model_call_log_id_seq`. `ALTER DEFAULT
/// PRIVILEGES` does not cover that sequence: 0005 creates it before 0006 sets
/// the defaults. The insert here runs under `SET ROLE cadus_admin`, so the
/// sequence grant of that role is still under test.
///
/// Finding #5: the table carries `user_id`, `session_id`, token counts, and
/// `cost_usd`, and it stays outside row-level security, so a table-wide grant
/// gave one tenant every tenant's rows and a one-statement wipe of the T6
/// ledger. T2 names the worker as the only unit that spends tokens, and the
/// worker connects as `cadus_admin`.
#[tokio::test]
async fn admin_role_inserts_into_model_call_log() {
    TestDb::with(|db| async move {
        // One fixed connection: SET ROLE outlives a statement, so the test
        // returns the connection to the pool with RESET ROLE.
        let mut conn = db.admin.acquire().await.unwrap();
        sqlx::query("SET ROLE cadus_admin")
            .execute(&mut *conn)
            .await
            .unwrap();

        let id = sqlx::query_scalar!(
            "INSERT INTO model_call_log (purpose, model_id, latency_ms)
             VALUES ('test', 'none', 1)
             RETURNING id"
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(id, 1);

        sqlx::query("RESET ROLE").execute(&mut *conn).await.unwrap();
        drop(conn);

        // #5: the runtime role writes no row and reads no row.
        let insert_err = sqlx::query!(
            "INSERT INTO model_call_log (purpose, model_id, latency_ms)
             VALUES ('test', 'none', 1)"
        )
        .execute(&db.app)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&insert_err), "42501");

        let select_err = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM model_call_log"#)
            .fetch_one(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&select_err), "42501");
    })
    .await;
}

/// C6, finding #14: the runtime role reads `content_store` and never writes it.
///
/// `content_store` binds approval to the digest (`migrations/0005_content.sql`),
/// so an edited body is a new row that needs its own approval. Table-wide
/// INSERT, UPDATE, and DELETE let the request tier rewrite an approved body in
/// place and insert a row that already carried `status = 'approved'`.
#[tokio::test]
async fn app_role_reads_content_store_and_never_writes_it() {
    TestDb::with(|db| async move {
        sqlx::query!(
            "INSERT INTO content_store (digest, kp_id, kind, body, status)
             VALUES ('sha256:seed', 'kp.x', 'template',
                     '{\"statement\": \"reviewed\"}'::jsonb, 'approved')"
        )
        .execute(&db.admin)
        .await
        .unwrap();

        // The serve path reads approved content.
        let seen = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM content_store"#)
            .fetch_one(&db.app)
            .await
            .unwrap();
        assert_eq!(seen, 1);

        let update_err = sqlx::query!("UPDATE content_store SET body = '{}'::jsonb")
            .execute(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&update_err), "42501");

        let insert_err = sqlx::query!(
            "INSERT INTO content_store (digest, kp_id, kind, body, status)
             VALUES ('sha256:new', 'kp.x', 'template', '{}'::jsonb, 'approved')"
        )
        .execute(&db.app)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&insert_err), "42501");

        // The approved body is still the one the admin seeded.
        let statement = sqlx::query_scalar!(
            r#"SELECT body ->> 'statement' AS "statement!"
               FROM content_store WHERE digest = 'sha256:seed'"#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(statement, "reviewed");
    })
    .await;
}

/// Input-only INSERT privileges cannot mint a report result, worker lease, or status.
#[tokio::test]
async fn report_inputs_cannot_forge_worker_owned_columns() {
    TestDb::with(|db| async move {
        let user = db.seed_user("report-input-only@example.test").await;
        for (column, sql) in [
            ("id", "INSERT INTO problem_reports
                (user_id,request_id,task_id,problem_id,attempt_id,source_hash,input,id)
                SELECT $1,gen_random_uuid(),'task','problem','attempt',repeat('a',64),'{}'::jsonb,gen_random_uuid()
                WHERE false"),
            ("status", "INSERT INTO problem_reports
                (user_id,request_id,task_id,problem_id,attempt_id,source_hash,input,status)
                SELECT $1,gen_random_uuid(),'task','problem','attempt',repeat('a',64),'{}'::jsonb,'completed'
                WHERE false"),
            ("stage", "INSERT INTO problem_reports
                (user_id,request_id,task_id,problem_id,attempt_id,source_hash,input,stage)
                SELECT $1,gen_random_uuid(),'task','problem','attempt',repeat('a',64),'{}'::jsonb,'published'
                WHERE false"),
            ("attempt", "INSERT INTO problem_reports
                (user_id,request_id,task_id,problem_id,attempt_id,source_hash,input,attempt)
                SELECT $1,gen_random_uuid(),'task','problem','attempt',repeat('a',64),'{}'::jsonb,3
                WHERE false"),
            ("lease", "INSERT INTO problem_reports
                (user_id,request_id,task_id,problem_id,attempt_id,source_hash,input,lease)
                SELECT $1,gen_random_uuid(),'task','problem','attempt',repeat('a',64),'{}'::jsonb,gen_random_uuid()
                WHERE false"),
            ("lease_until", "INSERT INTO problem_reports
                (user_id,request_id,task_id,problem_id,attempt_id,source_hash,input,lease_until)
                SELECT $1,gen_random_uuid(),'task','problem','attempt',repeat('a',64),'{}'::jsonb,now()
                WHERE false"),
            ("result", "INSERT INTO problem_reports
                (user_id,request_id,task_id,problem_id,attempt_id,source_hash,input,result)
                SELECT $1,gen_random_uuid(),'task','problem','attempt',repeat('a',64),'{}'::jsonb,'{}'::jsonb
                WHERE false"),
            ("created_at", "INSERT INTO problem_reports
                (user_id,request_id,task_id,problem_id,attempt_id,source_hash,input,created_at)
                SELECT $1,gen_random_uuid(),'task','problem','attempt',repeat('a',64),'{}'::jsonb,now()
                WHERE false"),
            ("updated_at", "INSERT INTO problem_reports
                (user_id,request_id,task_id,problem_id,attempt_id,source_hash,input,updated_at)
                SELECT $1,gen_random_uuid(),'task','problem','attempt',repeat('a',64),'{}'::jsonb,now()
                WHERE false"),
        ] {
            let mut tx = begin_tenant(&db.app, user).await.unwrap();
            let denied = sqlx::query(sql).bind(user).execute(&mut *tx).await.unwrap_err();
            assert_eq!(sqlstate(&denied), "42501", "forged column {column}");
            tx.rollback().await.unwrap();
        }
        for sql in [
            "UPDATE problem_reports SET result = '{}'::jsonb",
            "DELETE FROM problem_reports",
            "TRUNCATE problem_reports",
        ] {
            let mut tx = begin_tenant(&db.app, user).await.unwrap();
            let denied = sqlx::query(sql).execute(&mut *tx).await.unwrap_err();
            assert_eq!(sqlstate(&denied), "42501", "{sql}");
            tx.rollback().await.unwrap();
        }
    }).await;
}

/// BYPASSRLS does not grant the worker UPDATE, DELETE, or TRUNCATE on immutable evidence.
#[tokio::test]
async fn report_evidence_and_corrections_are_append_only_for_admin() {
    TestDb::with(|db| async move {
        for sql in [
            "UPDATE problem_report_steps SET evidence = '{}'::jsonb",
            "DELETE FROM problem_report_steps",
            "TRUNCATE problem_report_steps",
            "UPDATE problem_corrections SET body = '{}'::jsonb",
            "DELETE FROM problem_corrections",
            "TRUNCATE problem_corrections",
        ] {
            let mut tx = db.admin.begin().await.unwrap();
            sqlx::query("SET LOCAL ROLE cadus_admin").execute(&mut *tx).await.unwrap();
            let denied = sqlx::query(sql).execute(&mut *tx).await.unwrap_err();
            assert_eq!(sqlstate(&denied), "42501", "{sql}");
            tx.rollback().await.unwrap();
        }
    }).await;
}
