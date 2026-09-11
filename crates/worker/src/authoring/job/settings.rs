//! Configuration of one bounded offline author job.
use super::{AUTHORING_ATTEMPTS, AuthoringJob};
use cadus_model_client::Client;
impl AuthoringJob {
    /// Fill only completely absent pending/approved content pairs.
    #[must_use]
    pub const fn with_missing_only(mut self, missing_only: bool) -> Self {
        self.missing_only = missing_only;
        self
    }

    /// Select the provider-portable JSON-string transport and decline artifacts.
    #[must_use]
    pub fn with_transport(mut self, portable: bool, decline_dir: Option<String>) -> Self {
        self.portable_schema = portable;
        self.decline_dir = decline_dir.map(std::path::PathBuf::from);
        self
    }

    /// The permanent HTTP rejection that stopped this shared job.
    #[must_use]
    pub fn endpoint_failure(&self) -> Option<u16> {
        let status = self
            .endpoint_status
            .load(std::sync::atomic::Ordering::SeqCst);
        (status != 0).then_some(status)
    }

    /// Share one reservation cap across every kind and concurrent request.
    #[must_use]
    pub fn with_budget(mut self, budget: crate::authoring::budget::Budget) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Build the job around a client, with the [`AUTHORING_ATTEMPTS`] bound.
    #[must_use]
    pub const fn new(client: Client) -> Self {
        Self {
            client,
            attempts: AUTHORING_ATTEMPTS,
            budget: None,
            endpoint_status: std::sync::atomic::AtomicU16::new(0),
            portable_schema: false,
            decline_dir: None,
            missing_only: false,
        }
    }

    /// Build the job with another attempt bound.
    ///
    /// A bound of 0 makes no call and declines at once, which is what the dry
    /// run of unit R8 wants.
    #[must_use]
    pub const fn with_attempts(client: Client, attempts: u32) -> Self {
        Self {
            client,
            attempts,
            budget: None,
            endpoint_status: std::sync::atomic::AtomicU16::new(0),
            portable_schema: false,
            decline_dir: None,
            missing_only: false,
        }
    }
}
