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
//! Unit U11 adds the four series of spec section 7. They come from TWO sources,
//! and the split is the design:
//!
//! - [`GRADE_METRIC`] and the `ready_preauthored` label of [`DIAGNOSIS_METRIC`]
//!   count DECISIONS this process took. They live in [`Registry`], they start at
//!   zero on every boot, and a replayed request counts its decision again.
//! - [`MODEL_TOKENS_METRIC`], [`MODEL_LATENCY_METRIC`] and the other labels of
//!   [`DIAGNOSIS_METRIC`] count ROWS. The model calls run in `cadus-worker`, a
//!   different process, so a counter in this one reports zero for ever.
//!   The scrape reads them from the two tables the worker writes, through the
//!   SECURITY DEFINER aggregates of `migrations/0009_metrics_readers.sql`
//!   ([`ledger_totals`]). They survive a restart of either process, and they
//!   stay correct with more than one worker.
//!
//! `cadus_app` reaches no ROW of `model_call_log` and no row of `diagnosis_jobs`
//! through those functions: what comes back is one line per `purpose` and one
//! line per job `status`, with no tenant, no session and no money in it.
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
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::grade::{Grade, TAG_BLANK_ANSWER, TAG_NOTATION};

mod ledger;

pub use ledger::{LedgerTotals, PurposeTotals, ledger_totals, render_ledger};

/// The route label of a path that matched no route.
pub const UNMATCHED_ROUTE: &str = "__unmatched__";

/// The method label of a request method outside [`KNOWN_METHODS`].
pub const OTHER_METHOD: &str = "__other__";

/// The name of the request counter.
pub const REQUESTS_METRIC: &str = "cadus_http_requests_total";

/// The name of the request-latency histogram.
pub const DURATION_METRIC: &str = "cadus_http_request_duration_seconds";

/// The name of the deterministic-grade counter (1.0: `metrics.py:109-114`).
pub const GRADE_METRIC: &str = "cadus_deterministic_grade_total";

/// The name of the diagnosis-job counter.
pub const DIAGNOSIS_METRIC: &str = "cadus_diagnosis_jobs_total";

/// The name of the model-call token counter (T6).
pub const MODEL_TOKENS_METRIC: &str = "cadus_model_call_tokens_total";

/// The name of the model-call latency summary (T6).
pub const MODEL_LATENCY_METRIC: &str = "cadus_model_call_latency_seconds";

/// Every `result` label of [`GRADE_METRIC`], rendered whether it counted or not.
///
/// The full label set ships even at zero, because a dashboard that silently
/// misses a series reads as a service that never took that decision.
///
/// - `correct`: the answer is the authored answer.
/// - `notation`: correct, written in a form the checker names (a period-grouped
///   integer). It is a pass, counted apart, so the share of near-misses in FORM
///   is visible.
/// - `blank`: nothing was submitted.
/// - `incorrect`: a deterministic miss. An answer outside the grammar is one of
///   these (spec section 5.1), not a third thing.
/// - `undecidable`: the answer kind carries no deterministic verdict, so the
///   route answers `409 undecidable_kind` and this service asks no model.
pub const GRADE_RESULTS: [&str; 5] = ["correct", "notation", "blank", "incorrect", "undecidable"];

/// The `result` label of a correct answer.
pub const GRADE_CORRECT: &str = "correct";

/// The `result` label of a correct answer in a named form.
pub const GRADE_NOTATION: &str = "notation";

/// The `result` label of a blank submission.
pub const GRADE_BLANK: &str = "blank";

/// The `result` label of a deterministic miss.
pub const GRADE_INCORRECT: &str = "incorrect";

/// The `result` label of an answer kind with no deterministic verdict.
pub const GRADE_UNDECIDABLE: &str = "undecidable";

/// The `result` label of a diagnosis served from pre-authored content.
///
/// A pre-authored hit writes NO job row (spec section 6.2), so this label has no
/// row to be counted from and lives in [`Registry`] instead.
pub const DIAGNOSIS_PREAUTHORED: &str = "ready_preauthored";

/// The `result` label of every job row the queue holds.
pub const DIAGNOSIS_ENQUEUED: &str = "enqueued";

/// The job statuses [`DIAGNOSIS_METRIC`] reports one by one, and the `result`
/// label each one carries. `pending` and `running` are not final, so they count
/// under [`DIAGNOSIS_ENQUEUED`] only.
pub const DIAGNOSIS_STATUSES: [&str; 3] = ["done", "failed", "capped"];

/// The `kind` labels of [`MODEL_TOKENS_METRIC`].
pub const TOKEN_KINDS: [&str; 4] = ["cached", "uncached", "output", "reasoning"];

/// The `purpose` labels that ship even at zero (T2 names no third spender).
pub const PURPOSES: [&str; 2] = ["authoring", "diagnosis"];

/// The bound on the two ledger reads of one scrape.
///
/// A scrape must answer fast even when the datastore does not. The client bound
/// of [`Db`] is 10 s by default and a Prometheus scrape gives up at 10 s, so a
/// datastore that hangs then costs the operator the request series as well as
/// the ledger series. This bound cuts the ledger read first, and the rest of the
/// scrape still answers. Both reads are one aggregate over two small tables, so
/// a healthy datastore is three orders of magnitude inside it.
pub const LEDGER_READ_BOUND: Duration = Duration::from_millis(2_000);

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
    counters: Mutex<BTreeMap<(&'static str, &'static str), u64>>,
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
        self.series.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Take the counter lock. The same rule as [`Registry::lock`]: a poisoned
    /// lock gives its data back, because dropped metrics must not take the
    /// process with them.
    fn counter_lock(&self) -> MutexGuard<'_, BTreeMap<(&'static str, &'static str), u64>> {
        self.counters.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Count one deterministic grade decision (spec section 7).
    ///
    /// `result` is one of [`GRADE_RESULTS`]. The count is per DECISION: a
    /// replayed request grades the same submission again and counts again,
    /// exactly as 1.0 counts at its own grade call.
    pub fn count_grade(&self, result: &'static str) {
        *self
            .counter_lock()
            .entry((GRADE_METRIC, result))
            .or_default() += 1;
    }

    /// Count one diagnosis decision this process took.
    ///
    /// The only label the request tier owns is [`DIAGNOSIS_PREAUTHORED`]: a
    /// pre-authored hit writes no job row, so no row can be counted for it. The
    /// other labels come from the queue itself ([`ledger_totals`]).
    pub fn count_diagnosis(&self, result: &'static str) {
        *self
            .counter_lock()
            .entry((DIAGNOSIS_METRIC, result))
            .or_default() += 1;
    }

    /// The current value of one counter. For the tests.
    pub fn counter_of(&self, metric: &str, label: &str) -> u64 {
        self.counter_lock()
            .iter()
            .find(|((m, l), _)| *m == metric && *l == label)
            .map_or(0, |(_, count)| *count)
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
        drop(series);

        out.push_str(&format!(
            "# HELP {GRADE_METRIC} Grading decisions taken with no model call, by result.\n\
             # TYPE {GRADE_METRIC} counter\n"
        ));
        for result in GRADE_RESULTS {
            out.push_str(&format!(
                "{GRADE_METRIC}{{result=\"{result}\"}} {}\n",
                self.counter_of(GRADE_METRIC, result)
            ));
        }
        out
    }
}
/// The `result` label of one deterministic grade (spec section 7).
///
/// The three server-produced tags decide it: a blank submission carries
/// [`TAG_BLANK_ANSWER`] and a correct answer in a named form carries
/// [`TAG_NOTATION`]. An answer outside the grammar is a MISS with no tag (spec
/// section 5.1), so it counts as [`GRADE_INCORRECT`], which is what it is.
/// [`GRADE_UNDECIDABLE`] belongs to the answer KIND, not to the answer: the
/// route counts it where it refuses the kind.
#[must_use]
pub fn grade_result(grade: &Grade) -> &'static str {
    if grade.error_tags.iter().any(|tag| tag == TAG_BLANK_ANSWER) {
        return GRADE_BLANK;
    }
    if !grade.correct {
        return GRADE_INCORRECT;
    }
    if grade.error_tags.iter().any(|tag| tag == TAG_NOTATION) {
        GRADE_NOTATION
    } else {
        GRADE_CORRECT
    }
}

/// Escape a label value for the exposition format.
///
/// A backslash, a double quote, and a newline are the three characters the
/// format escapes. No label this crate writes can hold one — a route template is
/// a literal in the source and a method label comes from [`method_label`] — so
/// this is the belt that keeps a later label from breaking the whole scrape.
pub(super) fn escape(value: &str) -> String {
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
///
/// The request series and the grade counter come from this process. The three
/// ledger-backed series of spec section 7 come from the two tables the worker
/// writes, so the scrape does one read of each. A failed read drops those series
/// from this scrape and never fails the endpoint: an operator who cannot see the
/// token counters must still see the request counters.
pub async fn scrape(State(state): State<AppState>) -> Response {
    let mut body = state.metrics.render();
    if let Some(totals) = ledger_totals(&state.db).await {
        body.push_str(&render_ledger(
            &totals,
            state
                .metrics
                .counter_of(DIAGNOSIS_METRIC, DIAGNOSIS_PREAUTHORED),
        ));
    }
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static(EXPOSITION_CONTENT_TYPE),
        )],
        body,
    )
        .into_response()
}
