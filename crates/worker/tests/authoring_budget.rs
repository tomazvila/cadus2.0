//! A shared reservation cap precedes model requests and every retry (T3).
#![allow(clippy::unwrap_used)]
mod common;
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::{
    budget::{Budget, usd_micros},
    cli::{Command, parse},
    job::{AuthoringJob, author_one, run_parallel},
    prompt::Kind,
};
use common::{FakeModel, squares_spec};

#[test]
fn money_is_exact_and_parallel_reservations_never_exceed_the_cap() {
    assert_eq!(usd_micros("5.000001").unwrap(), 5_000_001);
    for bad in ["-1", "NaN", "1e3", "0.0000001", "18446744073709551615"] {
        assert!(usd_micros(bad).is_err());
    }
    let cap = Budget::new(100, 3, 16000).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..10 {
            let cap = &cap;
            scope.spawn(move || {
                for _ in 0..20 {
                    let _ = cap.reserve(16000, 100);
                }
            });
        }
    });
    assert_eq!(cap.reserved_micros(), 99);
    assert!(cap.reserve(16000, 100).is_err());
    assert!(Budget::new(5, 0, 1).is_err());
    assert!(Budget::new(5, 6, 1).is_err());
    let cap = Budget::new(100, 1, 4000).unwrap();
    assert!(cap.reserve(16000, 100).is_err());
    assert_eq!(cap.reserved_micros(), 0);
    assert!(cap.reserve(4000, 65_537).is_err());
}

#[test]
fn cli_exposes_explicit_cap_and_bounded_concurrency() {
    let Command::Author(args) = parse(&[
        "author",
        "--budget-usd",
        "5",
        "--request-reserve-usd",
        "0.1",
        "--concurrency",
        "4",
    ])
    .unwrap() else {
        unreachable!()
    };
    assert_eq!(args.budget_micros, Some(5_000_000));
    assert_eq!(args.request_reserve_micros, Some(100_000));
    assert_eq!(args.concurrency, 4);
    assert!(parse(&["author", "--concurrency", "17"]).is_err());
}

#[tokio::test]
async fn transport_retry_and_author_retry_share_the_same_cap() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![(500, "{}".to_owned()); 10]).await;
        let budget = Budget::new(3, 1, 16000).unwrap();
        let job = AuthoringJob::new(fake.client(4000, 2000)).with_budget(budget.clone());
        let report = author_one(&common::handle(&db), &job, Kind::Template, &squares_spec())
            .await
            .unwrap();
        assert_eq!(fake.call_count(), 3);
        assert_eq!(report.http_attempts.len(), 3);
        assert_eq!(budget.reserved_micros(), 3);
        assert!(
            report
                .decline
                .unwrap()
                .reasons
                .iter()
                .any(|reason| reason.contains("budget exhausted"))
        );
        let again = author_one(&common::handle(&db), &job, Kind::Template, &squares_spec())
            .await
            .unwrap();
        assert_eq!(again.attempts, 0);
        assert_eq!(fake.call_count(), 3);
    })
    .await;
}

#[tokio::test]
async fn parallel_knowledge_points_share_one_cap_and_duplicate_keys_make_no_call() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![(400, "{}".to_owned()); 10]).await;
        let budget = Budget::new(3, 1, 16000).unwrap();
        let job = AuthoringJob::new(fake.client(4000, 2000)).with_budget(budget.clone());
        let specs: Vec<_> = (0..8)
            .map(|n| {
                let mut spec = squares_spec();
                spec.kp_id = format!("kp{n}");
                spec
            })
            .collect();
        let batch = run_parallel(&common::handle(&db), &job, Kind::Template, &specs, 4)
            .await
            .unwrap();
        assert_eq!(batch.stored, 0);
        assert_eq!(fake.call_count(), 3);
        assert_eq!(budget.reserved_micros(), 3);
        assert!(
            run_parallel(
                &common::handle(&db),
                &job,
                Kind::Template,
                &[specs[0].clone(), specs[0].clone()],
                4
            )
            .await
            .is_err()
        );
        assert_eq!(fake.call_count(), 3);
    })
    .await;
}

#[tokio::test]
async fn provider_price_overrun_blocks_the_transport_retry_before_it_opens_a_socket() {
    TestDb::with(|db| async move {
        let body = serde_json::json!({"usage": {"cost": 0.2}, "choices": []}).to_string();
        let fake = FakeModel::start(vec![(200, body)]).await;
        let budget = Budget::new(5_000_000, 100_000, 16000).unwrap();
        let job = AuthoringJob::new(fake.client(4000, 2000)).with_budget(budget.clone());
        let report = author_one(&common::handle(&db), &job, Kind::Template, &squares_spec())
            .await
            .unwrap();
        assert_eq!(fake.call_count(), 1);
        assert_eq!(report.http_attempts.len(), 1);
        assert!(budget.breached());
        assert_eq!(budget.reported_micros(), 200_000);
    })
    .await;
}

#[tokio::test]
async fn three_workers_overlap_and_never_open_a_fourth_request() {
    use cadus_testkit::http::{read_request, status_reply};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    use tokio::{io::AsyncWriteExt, net::TcpListener};
    TestDb::with(|db| async move {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let seen_peak = peak.clone();
        let endpoint = tokio::spawn(async move {
            let mut replies = tokio::task::JoinSet::new();
            for _ in 0..6 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let active = active.clone();
                let peak = peak.clone();
                replies.spawn(async move {
                    let _ = read_request(&mut socket).await;
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    let (status, body) = common::tool_reply(&common::good_arguments());
                    socket
                        .write_all(status_reply(status, &body).as_bytes())
                        .await
                        .unwrap();
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
            while let Some(result) = replies.join_next().await {
                result.unwrap();
            }
        });
        let cfg = cadus_model_client::ModelConfig {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            api_key: "test".to_owned(),
            model: "test".to_owned(),
            output_tokens: 4000,
            reasoning_max_tokens: 2000,
            provider_order: Vec::new(),
            timeout: Duration::from_secs(5),
        };
        let budget = Budget::new(600_000, 100_000, 16000).unwrap();
        let job = AuthoringJob::with_attempts(cadus_model_client::Client::new(cfg).unwrap(), 1)
            .with_budget(budget.clone());
        let specs: Vec<_> = (0..6)
            .map(|n| {
                let mut s = squares_spec();
                s.kp_id = format!("overlap{n}");
                s
            })
            .collect();
        let report = run_parallel(&common::handle(&db), &job, Kind::Template, &specs, 3)
            .await
            .unwrap();
        assert_eq!(report.stored, 6);
        endpoint.await.unwrap();
        assert_eq!(seen_peak.load(Ordering::SeqCst), 3);
        assert_eq!(budget.reserved_micros(), 600_000);
    })
    .await;
}

#[test]
fn routing_price_caps_and_known_cost_refunds_preserve_the_shared_limit() {
    let budget = Budget::new(1_000_000, 500_000, 16000).unwrap();
    let mut body =
        serde_json::json!({"provider": {"order": ["DeepSeek"], "allow_fallbacks": true}});
    budget.prepare(&mut body, 16000).unwrap();
    assert_eq!(body["provider"]["allow_fallbacks"], false);
    assert_eq!(
        body["provider"]["max_price"],
        serde_json::json!({"prompt": 1.91, "completion": 3.83, "request": 0})
    );
    let mut attempt = common::one_attempt();
    attempt.cost_usd = Some("0.0123451".to_owned());
    budget.observe(&attempt);
    assert_eq!(budget.reserved_micros(), 12_346);
    assert_eq!(budget.reported_micros(), 12_346);
    budget.prepare(&mut body, 16000).unwrap();
    attempt.cost_usd = Some("4e-7".to_owned());
    budget.observe(&attempt);
    assert_eq!(budget.reserved_micros(), 12_347);
    assert!(!budget.breached());
    assert!(
        Budget::new(1_000_000, 1, 16000)
            .unwrap()
            .prepare(&mut body, 16000)
            .is_err()
    );
}
