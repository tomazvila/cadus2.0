//! Hard reservation limits for offline author passes (T3).
//!
//! The operator supplies an upper cost bound per HTTP request. It must cover
//! input, the largest output ceiling, provider fees, and route fallbacks.
//! Confirmed prices return unused reserves. A timeout retains its full reserve.
//! This cap bounds reservations, conditional on that provider price bound. Use a
//! provider-side credit limit as the independent actual-charge ceiling.

use crate::WorkerError;
use cadus_model_client::{Attempt, ModelError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// One process-wide ledger, shared across kinds, retries and concurrent tasks.
#[derive(Debug, Clone)]
pub struct Budget {
    reserved: Arc<AtomicU64>,
    breached: Arc<AtomicBool>,
    refused: Arc<AtomicBool>,
    reported: Arc<AtomicU64>,
    limit: u64,
    per_request: u64,
    max_tokens: u32,
}

impl Budget {
    /// Build a cap in millionths of a dollar.
    ///
    /// # Errors
    /// Reject zero request reserves, reserves above the cap, and zero tokens.
    pub fn new(limit: u64, per_request: u64, max_tokens: u32) -> Result<Self, WorkerError> {
        if per_request == 0 || per_request > limit || max_tokens == 0 {
            return Err(WorkerError::Config(
                "budget needs 0 < request reserve <= budget and a positive output ceiling"
                    .to_owned(),
            ));
        }
        Ok(Self {
            reserved: Arc::new(AtomicU64::new(0)),
            breached: Arc::new(AtomicBool::new(false)),
            refused: Arc::new(AtomicBool::new(false)),
            reported: Arc::new(AtomicU64::new(0)),
            limit,
            per_request,
            max_tokens,
        })
    }

    /// Apply route price caps and reserve the exact serialized request.
    ///
    /// # Errors
    /// Return the same refusals as `reserve`.
    pub fn prepare(&self, body: &mut serde_json::Value, max_tokens: u32) -> Result<(), ModelError> {
        if let Some(provider) = body
            .get_mut("provider")
            .and_then(serde_json::Value::as_object_mut)
        {
            provider.insert(
                "max_price".to_owned(),
                serde_json::json!({"prompt": 1.91, "completion": 3.83, "request": 0}),
            );
            provider.insert("allow_fallbacks".to_owned(), serde_json::json!(false));
            provider.insert("sort".to_owned(), serde_json::json!("throughput"));
        }
        let bytes = body.to_string().len();
        if body.get("provider").is_some() {
            let input = u64::try_from(bytes)
                .unwrap_or(u64::MAX)
                .saturating_add(4096);
            let required = input
                .saturating_mul(191)
                .saturating_add(u64::from(max_tokens) * 383)
                .div_ceil(100);
            if self.per_request < required {
                return Err(ModelError::Config(
                    "request reserve is below the enforced route-price and token bound".to_owned(),
                ));
            }
        }
        self.reserve(max_tokens, bytes)
    }

    /// Reserve before each HTTP request; no retry or parallel task bypasses this.
    ///
    /// # Errors
    /// Refuse an output ceiling above the priced ceiling or an exhausted balance.
    pub fn reserve(&self, max_tokens: u32, request_bytes: usize) -> Result<(), ModelError> {
        if self.breached.load(Ordering::SeqCst) {
            return Err(ModelError::Config(
                "provider cost exceeded its reservation; author pass stopped".to_owned(),
            ));
        }
        if request_bytes > 65_536 {
            return Err(ModelError::Config(
                "authoring request exceeds the priced 65536-byte input bound".to_owned(),
            ));
        }
        if max_tokens > self.max_tokens || max_tokens > 16_000 {
            return Err(ModelError::Config(
                "authoring output exceeds the reserved request ceiling".to_owned(),
            ));
        }
        self.reserved
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |spent| {
                spent
                    .checked_add(self.per_request)
                    .filter(|next| *next <= self.limit)
            })
            .map(|_| ())
            .map_err(|_| {
                self.refused.store(true, Ordering::SeqCst);
                ModelError::Config(
                    "authoring reservation budget exhausted; no HTTP request sent".to_owned(),
                )
            })
    }

    /// Reconcile each HTTP reply before a transport retry starts.
    /// A price above its reserve blocks all later requests. In-flight requests drain.
    /// An absent price retains the full reserve. An invalid price stops the pass.
    pub fn observe(&self, attempt: &Attempt) {
        let Some(text) = &attempt.cost_usd else {
            return;
        };
        match reported_micros(text) {
            Ok(amount) => {
                if self
                    .reported
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |total| {
                        total.checked_add(amount)
                    })
                    .is_err()
                {
                    self.breached.store(true, Ordering::SeqCst);
                }
                if amount > self.per_request {
                    self.breached.store(true, Ordering::SeqCst);
                } else {
                    self.reserved
                        .fetch_sub(self.per_request - amount, Ordering::SeqCst);
                }
            }
            Err(_) => self.breached.store(true, Ordering::SeqCst),
        }
    }

    /// Whether any request exhausted the reservation balance.
    #[must_use]
    pub fn refused(&self) -> bool {
        self.refused.load(Ordering::SeqCst)
    }

    /// The sum of provider prices that fit the six-decimal ledger.
    #[must_use]
    pub fn reported_micros(&self) -> u64 {
        self.reported.load(Ordering::SeqCst)
    }

    /// Whether the provider price invalidated its configured bound.
    #[must_use]
    pub fn breached(&self) -> bool {
        self.breached.load(Ordering::SeqCst)
    }

    /// Known charges plus full reserves for unknown prices and active requests.
    #[must_use]
    pub fn reserved_micros(&self) -> u64 {
        self.reserved.load(Ordering::SeqCst)
    }
}

/// Parse USD exactly, with up to six decimal places.
///
/// # Errors
/// Reject signs, exponents, missing digits, excess precision and overflow.
pub fn usd_micros(text: &str) -> Result<u64, String> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 6
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(
            "USD must be an unsigned decimal with at most six fractional digits".to_owned(),
        );
    }
    let whole = whole
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(1_000_000));
    let fraction = format!("{fraction:0<6}").parse::<u64>().ok();
    whole
        .zip(fraction)
        .and_then(|(w, f)| w.checked_add(f))
        .ok_or_else(|| "USD amount overflows".to_owned())
}

/// Round a nonnegative provider decimal up to the ledger precision.
fn reported_micros(text: &str) -> Result<u64, String> {
    let (base, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
    let exponent: i32 = exponent.parse().map_err(|_| "invalid provider price")?;
    let (whole, fraction) = base.split_once('.').unwrap_or((base, ""));
    let digits = format!("{whole}{fraction}");
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid provider price".to_owned());
    }
    let coefficient = digits
        .parse::<u128>()
        .map_err(|_| "provider price overflow")?;
    let shift = exponent
        .checked_add(6)
        .and_then(|n| n.checked_sub(i32::try_from(fraction.len()).ok()?))
        .ok_or("provider price overflow")?;
    let magnitude = 10_u128
        .checked_pow(shift.unsigned_abs())
        .ok_or("provider price overflow")?;
    let micros = if shift >= 0 {
        coefficient
            .checked_mul(magnitude)
            .ok_or("provider price overflow")?
    } else {
        coefficient.div_ceil(magnitude)
    };
    u64::try_from(micros).map_err(|_| "provider price overflow".to_owned())
}

/// Report partial paid-pass results and return a nonzero CLI outcome on a failure.
///
/// # Errors
/// Refuse endpoint failures, price breaches, reservation denial and all declines.
pub fn finish(
    budget: &Budget,
    job: &crate::authoring::job::AuthoringJob,
    stored: u32,
    declined: u32,
) -> Result<(), WorkerError> {
    println!(
        "reserved: {} micro-USD; reported: {} micro-USD; price-bound breach: {}; stored {stored}; declined {declined}",
        budget.reserved_micros(),
        budget.reported_micros(),
        budget.breached()
    );
    let reason = if let Some(status) = job.endpoint_failure() {
        Some(format!("permanent author endpoint failure HTTP {status}"))
    } else if budget.breached() {
        Some("provider price-bound breach".to_owned())
    } else if budget.refused() {
        Some("author reservation budget exhausted".to_owned())
    } else if stored == 0 && declined > 0 {
        Some("all requested author documents declined".to_owned())
    } else {
        None
    };
    if let Some(reason) = reason {
        return Err(WorkerError::Config(format!(
            "{reason}; partial result: stored {stored}, declined {declined}"
        )));
    }
    Ok(())
}
