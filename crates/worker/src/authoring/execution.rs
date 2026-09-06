//! Ordered bank rounds share one budget for the complete author pass.
use crate::{
    WorkerError,
    authoring::{
        budget::{self, Budget},
        cli::{self, AuthorArgs},
        job::{AuthoringJob, run_parallel},
        prompt::{AuthoringSpec, Kind},
    },
};
use cadus_store::Db;

/// Run each requested template round before the instruction kinds.
///
/// # Errors
/// Return database errors and the explicit partial-pass failure status.
pub async fn run(
    db: &Db,
    job: &AuthoringJob,
    budget: &Budget,
    specs: &[AuthoringSpec],
    kinds: &[Kind],
    args: &AuthorArgs,
) -> Result<(), WorkerError> {
    let mut stored = 0_u32;
    let mut declined = 0_u32;
    'kinds: for kind in kinds {
        let rounds = if *kind == Kind::Template {
            args.template_passes.max(1)
        } else {
            1
        };
        for round in 1..=rounds {
            println!("author stage {} round {round}/{rounds}", kind.as_str());
            let report = run_parallel(db, job, *kind, specs, args.concurrency.max(1)).await?;
            stored += report.stored;
            declined += report.declined;
            print!("{}", cli::render_batch(*kind, &report));
            if job.endpoint_failure().is_some() || budget.refused() {
                break 'kinds;
            }
        }
    }
    budget::finish(budget, job, stored, declined)
}
