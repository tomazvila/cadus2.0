//! Part of `tests/skeleton.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::skeleton::*;

// ---------------------------------------------------------------------------
// The request-metrics layer and /metrics
// ---------------------------------------------------------------------------

/// (7) The counter is labelled by the matched route TEMPLATE.
#[tokio::test]
async fn the_counter_is_labelled_by_the_route_template() {
    let app = offline_app();

    send(&app, get("/api/health")).await;
    send(&app, get("/api/health")).await;

    let text = scrape(&app).await;

    assert!(
        text.contains(
            "cadus_http_requests_total{method=\"GET\",route=\"/api/health\",status=\"200\"} 2\n"
        ),
        "the scrape carries no /api/health counter:\n{text}"
    );
}

/// (8) U1 acceptance: an unmatched path is ONE `__unmatched__` metric label.
///
/// Three different paths that match no route must fold into one series with the
/// count 3. A label taken from the raw path would give three series here, and a
/// scanner that walks a million paths would then mint a million.
#[tokio::test]
async fn an_unmatched_path_is_one_unmatched_metric_label() {
    let app = offline_app();

    send(&app, get("/nope-one")).await;
    send(&app, get("/nope-two")).await;
    send(&app, get("/deeper/nope/three")).await;

    let text = scrape(&app).await;

    let counter_lines: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("cadus_http_requests_total{"))
        .filter(|line| line.contains("__unmatched__"))
        .collect();

    assert_eq!(
        counter_lines,
        vec!["cadus_http_requests_total{method=\"GET\",route=\"__unmatched__\",status=\"404\"} 3"],
        "three unmatched paths must give exactly one series:\n{text}"
    );

    // No raw path ever reaches a label.
    for path in ["nope-one", "nope-two", "deeper"] {
        assert!(
            !text.contains(path),
            "the raw path {path} reached a metric label:\n{text}"
        );
    }
}

/// (9) The metrics layer counts the `403` of the CSRF layer.
///
/// It sits outside that layer for this reason: a deployment under a CSRF attack
/// must be able to see it in the metrics, and a refusal that no series counts is
/// invisible.
#[tokio::test]
async fn the_csrf_refusal_is_counted() {
    let app = offline_app();

    let (status, _headers, _body) = send(&app, cross_site_answer_post()).await;
    assert_eq!(status.as_u16(), 403);

    let text = scrape(&app).await;

    // Unit U8 added the route, so the refusal now carries its TEMPLATE. The
    // label is the template either way: the layer sits outside the CSRF layer
    // and reads the matched route, not the raw path.
    assert!(
        text.contains(
            "cadus_http_requests_total{method=\"POST\",route=\"/api/task/{task_id}/answer\",\
             status=\"403\"} 1\n"
        ),
        "the scrape carries no 403 counter:\n{text}"
    );
}

/// (10) A method outside the known set folds into one `__other__` label.
///
/// HTTP admits any token as a method, so the raw method is an unbounded label
/// and a scanner could mint one series per made-up verb.
#[tokio::test]
async fn an_unknown_method_is_one_other_label() {
    let app = offline_app();

    for verb in ["FROB", "BLORP"] {
        let request = Request::builder()
            .method(verb)
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();
        let (status, _headers, _body) = send(&app, request).await;
        assert_eq!(status.as_u16(), 405);
    }

    let text = scrape(&app).await;

    assert!(
        text.contains(
            "cadus_http_requests_total{method=\"__other__\",route=\"/api/health\",status=\"405\"} 2\n"
        ),
        "two unknown methods must give one series:\n{text}"
    );
    assert!(
        !text.contains("FROB"),
        "the raw method reached a label:\n{text}"
    );
    assert!(
        !text.contains("BLORP"),
        "the raw method reached a label:\n{text}"
    );
}

/// The twelve bucket bounds of the duration histogram, in order.
const DURATION_BUCKETS: [&str; 12] = [
    "0.005", "0.01", "0.025", "0.05", "0.1", "0.25", "0.5", "1", "2.5", "5", "10", "+Inf",
];

/// The count on the one line of `text` that starts with `prefix`.
///
/// Returns `None` when the text carries no such line, and `None` when the rest
/// of that line is not a count.
fn count_after(text: &str, prefix: &str) -> Option<u64> {
    let start = text.find(prefix)? + prefix.len();
    let rest = text.get(start..)?;
    let line = rest.split('\n').next()?;
    line.trim().parse().ok()
}

/// Assert the shape of the duration histogram of one method and one route.
///
/// The shape is the type line, all twelve bounds, a count that never falls from
/// one bound to the next, the literal `total` on `+Inf`, and the pair of summary
/// series. The counts of the LOWER bounds carry no literal: a bound under the
/// request time reads 0 and a bound over it reads the total, and WHERE that
/// step sits is the clock of the runner, never the shape of the exposition. A
/// literal `1` on `le="0.005"` therefore demands that one debug-build request
/// finishes inside 5 ms (M5 review 2, finding V12).
fn assert_duration_histogram(text: &str, method: &str, route: &str, total: u64) {
    assert!(
        text.contains("# TYPE cadus_http_request_duration_seconds histogram\n"),
        "the scrape carries no histogram type line:\n{text}"
    );
    let mut below = 0;
    for bound in DURATION_BUCKETS {
        let prefix = format!(
            "cadus_http_request_duration_seconds_bucket{{method=\"{method}\",route=\"{route}\",le=\"{bound}\"}} "
        );
        let count = count_after(text, &prefix)
            .unwrap_or_else(|| panic!("the scrape has no bucket {bound}:\n{text}"));
        assert!(
            count >= below,
            "bucket {bound} counts {count} under the {below} of the bound below it, and a \
             histogram bucket is cumulative:\n{text}"
        );
        below = count;
    }
    assert_eq!(
        below, total,
        "the +Inf bucket must count every observation:\n{text}"
    );
    assert_eq!(
        count_after(
            text,
            &format!(
                "cadus_http_request_duration_seconds_count{{method=\"{method}\",route=\"{route}\"}} "
            )
        ),
        Some(total),
        "the scrape carries no count series of {total}:\n{text}"
    );
    assert!(
        text.contains(&format!(
            "cadus_http_request_duration_seconds_sum{{method=\"{method}\",route=\"{route}\"}}"
        )),
        "the scrape carries no sum series:\n{text}"
    );
}

/// The exposition one `GET /api/health` of 7 ms writes.
///
/// The request is over the first bound and under the second, so `le="0.005"`
/// counts 0 and every higher bound counts the one observation. Every line is a
/// literal of this file.
const SLOW_REQUEST_EXPOSITION: &str = concat!(
    "# TYPE cadus_http_request_duration_seconds histogram\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"0.005\"} 0\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"0.01\"} 1\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"0.025\"} 1\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"0.05\"} 1\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"0.1\"} 1\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"0.25\"} 1\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"0.5\"} 1\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"1\"} 1\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"2.5\"} 1\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"5\"} 1\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"10\"} 1\n",
    "cadus_http_request_duration_seconds_bucket{method=\"GET\",route=\"/api/health\",le=\"+Inf\"} 1\n",
    "cadus_http_request_duration_seconds_sum{method=\"GET\",route=\"/api/health\"} 0.007\n",
    "cadus_http_request_duration_seconds_count{method=\"GET\",route=\"/api/health\"} 1\n",
);

/// (11) The histogram carries its type line, every bucket bound, and the pair of
/// summary series.
#[tokio::test]
async fn the_duration_histogram_carries_its_buckets_and_summary() {
    let app = offline_app();

    send(&app, get("/api/health")).await;
    let text = scrape(&app).await;

    assert_duration_histogram(&text, "GET", "/api/health", 1);
}

/// (11a) The shape check holds for a request slower than the first bound.
///
/// The subject of check (11) is the shape of the exposition, never the speed of
/// the runner. A check that pins the literal `1` on `le="0.005"` too demands
/// that one debug-build `GET /api/health` finishes inside 5 ms, and it fails
/// whenever the runner is busy (M5 review 2, finding V12).
#[test]
fn the_shape_check_holds_for_a_request_over_the_first_bound() {
    assert_duration_histogram(SLOW_REQUEST_EXPOSITION, "GET", "/api/health", 1);
}

/// (12) The histogram drops the status label and folds the statuses of one
/// route together.
///
/// 1.0 labels the latency histogram by method and route only, so a route that
/// answers two different statuses has ONE latency series with both counts in it.
///
/// The two POSTs below are the demonstration. `/api/auth/login` is a real route
/// since M5 U4, so both carry the route TEMPLATE as their label: the first is
/// `403 cross_origin_rejected` from the CSRF layer, the second is
/// `422 invalid_request` from the handler, and the one series counts 2. The two
/// GETs match no route at all, so they fold into the one `__unmatched__` label.
#[tokio::test]
async fn the_histogram_folds_the_statuses_of_one_route() {
    let app = offline_app();

    send(&app, get("/nope-one")).await;
    send(&app, get("/nope-two")).await;

    let refused = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("host", "tutor.example")
        .header("origin", "https://evil.example")
        .body(Body::empty())
        .unwrap();
    let (refused_status, _headers, _body) = send(&app, refused).await;

    let served = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("host", "tutor.example")
        .body(Body::empty())
        .unwrap();
    let (served_status, _headers, _body) = send(&app, served).await;

    let text = scrape(&app).await;

    assert_eq!(refused_status.as_u16(), 403);
    assert_eq!(served_status.as_u16(), 422);
    assert!(text.contains(
        "cadus_http_request_duration_seconds_count{method=\"GET\",route=\"__unmatched__\"} 2\n"
    ));
    assert!(
        text.contains(
            "cadus_http_request_duration_seconds_count{method=\"POST\",route=\"/api/auth/login\"} \
             2\n"
        ),
        "the two statuses of one route must fold into one latency series:\n{text}"
    );
}

/// (13) An empty registry still renders both HELP and TYPE lines.
///
/// A scrape of a process that served nothing must be valid exposition text, not
/// an empty body.
#[tokio::test]
async fn an_empty_registry_renders_both_metric_headers() {
    let app = offline_app();

    let text = scrape(&app).await;

    assert!(text.contains("# TYPE cadus_http_requests_total counter\n"));
    assert!(text.contains("# TYPE cadus_http_request_duration_seconds histogram\n"));
    assert!(
        !text.contains("cadus_http_requests_total{"),
        "the first scrape counts nothing yet:\n{text}"
    );
}
