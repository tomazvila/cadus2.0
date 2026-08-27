//! The request-metrics registry and the `/metrics` scrape endpoint.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 2, row `GET /metrics`,
//! and section 11, unit U1. Two series, both taken from 1.0
//! (`cadus_web/metrics.py:83-166`):
//!
//! - `cadus_http_requests_total`, a counter, labelled `method`, `route`,
//!   `status`.
//! - `cadus_http_request_duration_seconds`, a histogram, labelled `method` and
//!   `route`.
//!
//! **The label set is bounded, and that is the whole design.** `route` is the
//! matched route TEMPLATE — `/api/task/{task_id}/serve`, never
//! `/api/task/t-review-fractions/serve` — so the label space is the route set,
//! not the id space. A path that matches no route gets the one sentinel
//! [`UNMATCHED_ROUTE`], so a scanner that walks a million paths adds one series
//! and not a million. `method` gets the same treatment through
//! [`method_label`]: HTTP admits any token as a method, so an unknown one
//! becomes [`OTHER_METHOD`].
//!
//! The registry holds no third-party crate. The Prometheus text exposition
//! format is a few lines of text, and `cadus-web` keeps its dependency list
//! short on purpose (R4, and the pinned list in `tests/purity.rs`).

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;

/// The route label of a path that matched no route.
pub const UNMATCHED_ROUTE: &str = "__unmatched__";

/// The method label of a request method outside [`KNOWN_METHODS`].
pub const OTHER_METHOD: &str = "__other__";

/// The name of the request counter.
pub const REQUESTS_METRIC: &str = "cadus_http_requests_total";

/// The name of the request-latency histogram.
pub const DURATION_METRIC: &str = "cadus_http_request_duration_seconds";

/// The content type of the Prometheus text exposition format, version 0.0.4.
pub const EXPOSITION_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// The methods that get their own label. Anything else is [`OTHER_METHOD`].
pub const KNOWN_METHODS: [&str; 9] = [
    "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE", "CONNECT",
];

/// The bucket bounds of the latency histogram, in seconds.
///
/// The Prometheus default set. The L1 to L5 budgets of
/// `docs/reference/l1-budget.md` are 150 ms to 300 ms, so the 0.1, 0.25 and 0.5
/// bounds bracket every route budget of M5.
pub const DURATION_BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// The counts of one `(method, route, status)` combination.
#[derive(Debug, Clone, Default)]
struct Series {
    /// How many requests landed here.
    count: u64,
    /// The sum of their latencies, in seconds.
    sum: f64,
    /// How many landed in each bucket of [`DURATION_BUCKETS`], not cumulative.
    /// The render step accumulates them, because the exposition format wants
    /// cumulative counts.
    buckets: [u64; DURATION_BUCKETS.len()],
}

/// One label combination.
type Key = (&'static str, String, u16);

/// The metrics of one process.
///
/// One map serves both series. The counter reads a key whole; the histogram
/// sums the keys that share a `(method, route)` pair, which gives exactly the
/// two-label histogram of 1.0.
#[derive(Debug, Default)]
pub struct Registry {
    series: Mutex<BTreeMap<Key, Series>>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the lock.
    ///
    /// The code under the lock does arithmetic and string work only, so it never
    /// panics and the lock is never poisoned. A poisoned lock still gives its
    /// data back here, because dropped metrics must not take the process with
    /// them.
    fn lock(&self) -> MutexGuard<'_, BTreeMap<Key, Series>> {
        self.series
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Record one finished request.
    pub fn observe(&self, method: &'static str, route: &str, status: u16, seconds: f64) {
        let mut series = self.lock();
        let entry = series
            .entry((method, route.to_string(), status))
            .or_default();
        entry.count += 1;
        entry.sum += seconds;
        let bucket = DURATION_BUCKETS
            .iter()
            .position(|bound| seconds <= *bound)
            .unwrap_or(DURATION_BUCKETS.len());
        if let Some(slot) = entry.buckets.get_mut(bucket) {
            *slot += 1;
        }
    }

    /// How many requests one label combination counted. For the tests.
    pub fn count_of(&self, method: &str, route: &str, status: u16) -> u64 {
        self.lock()
            .iter()
            .find(|((m, r, s), _)| *m == method && r == route && *s == status)
            .map_or(0, |(_, series)| series.count)
    }

    /// Render every series in the Prometheus text exposition format.
    pub fn render(&self) -> String {
        let series = self.lock();
        let mut out = String::new();

        out.push_str(&format!(
            "# HELP {REQUESTS_METRIC} HTTP requests, by method, matched route template, and \
             response status code.\n# TYPE {REQUESTS_METRIC} counter\n"
        ));
        for ((method, route, status), entry) in series.iter() {
            out.push_str(&format!(
                "{REQUESTS_METRIC}{{method=\"{}\",route=\"{}\",status=\"{status}\"}} {}\n",
                escape(method),
                escape(route),
                entry.count
            ));
        }

        // The histogram drops the status label, so fold the keys that share a
        // (method, route) pair before the render.
        let mut folded: BTreeMap<(&'static str, &str), Series> = BTreeMap::new();
        for ((method, route, _status), entry) in series.iter() {
            let target = folded.entry((method, route.as_str())).or_default();
            target.count += entry.count;
            target.sum += entry.sum;
            for (slot, add) in target.buckets.iter_mut().zip(entry.buckets.iter()) {
                *slot += *add;
            }
        }

        out.push_str(&format!(
            "# HELP {DURATION_METRIC} HTTP request latency in seconds, by method and matched \
             route template.\n# TYPE {DURATION_METRIC} histogram\n"
        ));
        for ((method, route), entry) in folded {
            let labels = format!("method=\"{}\",route=\"{}\"", escape(method), escape(route));
            let mut cumulative = 0_u64;
            for (bound, count) in DURATION_BUCKETS.iter().zip(entry.buckets.iter()) {
                cumulative += *count;
                out.push_str(&format!(
                    "{DURATION_METRIC}_bucket{{{labels},le=\"{bound}\"}} {cumulative}\n"
                ));
            }
            out.push_str(&format!(
                "{DURATION_METRIC}_bucket{{{labels},le=\"+Inf\"}} {}\n{DURATION_METRIC}_sum{{{labels}}} {}\n{DURATION_METRIC}_count{{{labels}}} {}\n",
                entry.count, entry.sum, entry.count
            ));
        }
        out
    }
}

/// Escape a label value for the exposition format.
///
/// A backslash, a double quote, and a newline are the three characters the
/// format escapes. No label this crate writes can hold one — a route template is
/// a literal in the source and a method label comes from [`method_label`] — so
/// this is the belt that keeps a later label from breaking the whole scrape.
fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// The bounded label of a request method.
///
/// HTTP admits any token as a method, so the raw method is an unbounded label
/// and a scanner could mint one series per made-up verb.
pub fn method_label(method: &Method) -> &'static str {
    let raw = method.as_str();
    KNOWN_METHODS
        .into_iter()
        .find(|known| *known == raw)
        .unwrap_or(OTHER_METHOD)
}

/// The matched route TEMPLATE of a request, or [`UNMATCHED_ROUTE`].
///
/// axum's router puts `MatchedPath` in the request extensions before it calls
/// the layered service, so a layer added with `Router::layer` reads the template
/// and never the raw path. A request that matched no route carries no
/// `MatchedPath`, which is the sentinel case.
pub fn route_label(request: &Request) -> String {
    request
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| UNMATCHED_ROUTE.to_string(), |m| m.as_str().to_string())
}

/// The request-metrics layer.
///
/// It sits OUTSIDE the CSRF origin layer, so it counts the `403` that layer
/// answers, exactly as 1.0 does (`cadus_web/app.py:315`).
pub async fn request_metrics_layer(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let method = method_label(request.method());
    let route = route_label(&request);
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    state.metrics.observe(
        method,
        &route,
        response.status().as_u16(),
        started.elapsed().as_secs_f64(),
    );
    response
}

/// `GET /metrics`. The current value of every series, as Prometheus text.
pub async fn scrape(State(state): State<AppState>) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static(EXPOSITION_CONTENT_TYPE),
        )],
        state.metrics.render(),
    )
        .into_response()
}
