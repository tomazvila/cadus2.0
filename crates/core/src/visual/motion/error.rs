//! Why one `motion` object of a figure is refused.

use thiserror::Error;

/// A refusal of the `motion` object of a visual specification.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MotionError {
    /// The range of the parameter is empty, or it has more steps than allowed.
    #[error("the parameter range is wrong: {reason}")]
    ParameterRange {
        /// What is wrong.
        reason: String,
    },
    /// The step is zero, negative, or does not divide the range.
    #[error("the parameter step is wrong: {reason}")]
    ParameterStep {
        /// What is wrong.
        reason: String,
    },
    /// The default value does not sit on a step.
    #[error("the parameter default {default} does not sit on a step")]
    ParameterDefault {
        /// The authored default.
        default: String,
    },
    /// A bound, the step, or the default is not an integer or a terminating decimal.
    #[error("the parameter number {text:?} is not an integer or a terminating decimal")]
    ParameterNotDecimal {
        /// The authored text.
        text: String,
    },
    /// A drive does not fit the figure.
    #[error("the drive does not fit the figure: {reason}")]
    DriveMismatch {
        /// What does not fit.
        reason: String,
    },
    /// The figure moves more groups than the limit.
    #[error("the figure moves {groups} groups and the limit is {limit}")]
    TooManyGroups {
        /// The group count.
        groups: usize,
        /// The limit.
        limit: usize,
    },
    /// The motion table is larger than the limit.
    #[error("the motion table is {bytes} bytes and the limit is {limit}")]
    TableTooLarge {
        /// The size of the table as JSON.
        bytes: usize,
        /// The limit.
        limit: usize,
    },
}
