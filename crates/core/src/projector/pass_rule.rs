//! The lesson pass rule: the parsed form of `lesson.kp_pass` (D-F7, audit finding k).
//!
//! Grammar
//! -------
//!
//! ```text
//! rule := term ('|' term)*
//! term := <n> "consec" | <k> "of" <m>
//! ```
//!
//! `<n>consec` passes when the LAST `n` answers are correct. The `n` answers sit at the
//! tail of the sequence, not anywhere in it. `<k>of<m>` passes when at least `k` of the
//! FIRST `m` answers are correct. `|` is OR: one term that passes passes the rule. A
//! term needs `m` or more answers before it passes, so a short sequence fails.
//!
//! Every count is a decimal number of one or more, and `<k>of<m>` needs `k <= m`.
//! Whitespace around a term is not significant. Every other spelling is a
//! [`ConfigError`].
//!
//! The 1.0 default `"2consec|3of4"` gives the verdicts `projector.py:860-867` gives.

use crate::config::ConfigError;

/// One term of a [`PassRule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PassTerm {
    /// The last `n` answers are correct.
    Consecutive(usize),
    /// At least `correct` of the first `first` answers are correct.
    OfFirst {
        /// The number of correct answers the term asks for.
        correct: usize,
        /// The length of the head of the sequence the term reads.
        first: usize,
    },
}

impl PassTerm {
    /// Parse one term. Whitespace around it is not significant.
    fn parse(text: &str) -> Result<Self, ConfigError> {
        let term = text.trim();
        parse_consecutive(term)
            .or_else(|| parse_of_first(term))
            .ok_or_else(|| ConfigError::PassRuleTerm {
                term: term.to_owned(),
            })
    }

    /// Whether `seq` passes this term.
    fn passed(self, seq: &[bool]) -> bool {
        match self {
            Self::Consecutive(count) => {
                seq.len() >= count && seq[seq.len() - count..].iter().all(|answer| *answer)
            }
            Self::OfFirst { correct, first } => {
                seq.len() >= first
                    && seq.iter().take(first).filter(|answer| **answer).count() >= correct
            }
        }
    }
}

/// Parse `<n>consec`. `None` means the text is not that form.
fn parse_consecutive(term: &str) -> Option<PassTerm> {
    let digits = term.strip_suffix("consec")?;
    Some(PassTerm::Consecutive(parse_count(digits)?))
}

/// Parse `<k>of<m>`. `None` means the text is not that form.
fn parse_of_first(term: &str) -> Option<PassTerm> {
    let (head, tail) = term.split_once("of")?;
    let correct = parse_count(head)?;
    let first = parse_count(tail)?;
    if correct > first {
        return None;
    }
    Some(PassTerm::OfFirst { correct, first })
}

/// Parse a count of one or more. `None` means the text is not such a count: it holds a
/// character other than an ASCII digit, it is empty, it is over the `usize` range, or
/// it is zero.
fn parse_count(text: &str) -> Option<usize> {
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let count = text.parse::<usize>().ok()?;
    if count == 0 {
        return None;
    }
    Some(count)
}

/// The parsed form of `lesson.kp_pass` (D-F7).
///
/// The module documentation holds the grammar. [`PassRule::parse`] builds one, and
/// [`crate::projector::kp_passed`] reads one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassRule {
    /// The terms, in the order the string spells them. One term that passes passes
    /// the rule.
    terms: Vec<PassTerm>,
}

impl Default for PassRule {
    /// The 1.0 rule `"2consec|3of4"`.
    ///
    /// `crates/core/tests/projector_pass_rule.rs` pins that this value equals the
    /// parse of the default string.
    fn default() -> Self {
        Self {
            terms: vec![
                PassTerm::Consecutive(2),
                PassTerm::OfFirst {
                    correct: 3,
                    first: 4,
                },
            ],
        }
    }
}

impl PassRule {
    /// Parse a `lesson.kp_pass` string.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::EmptyPassRule`] for a string that holds no term, and
    /// [`ConfigError::PassRuleTerm`] for a term outside the grammar. The message of
    /// the second names the bad term.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        if text.trim().is_empty() {
            return Err(ConfigError::EmptyPassRule);
        }
        let mut terms = Vec::new();
        for part in text.split('|') {
            terms.push(PassTerm::parse(part)?);
        }
        Ok(Self { terms })
    }

    /// Whether `seq` passes the rule.
    ///
    /// `seq` is the answer sequence of ONE knowledge point, oldest first.
    #[must_use]
    pub fn passed(&self, seq: &[bool]) -> bool {
        self.terms.iter().any(|term| term.passed(seq))
    }
}
