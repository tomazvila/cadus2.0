//! The misconceptions the server can compute for one wrong answer.
//!
//! The model does not guess a mistake. The server lists the mistakes that explain the
//! learner's exact answer, and the model names the one that matches or says the mistake
//! could not be identified. The table is small and grows by kind of item: a rounding
//! slip, a reciprocal, a sign slip, and the wrong operation on the two fractions of a
//! problem.

use cadus_store::diagnosis::JobPayload;

/// One mistake that explains the learner's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The short name the model must echo in `misconception`.
    pub name: &'static str,
    /// What the learner did, as one plain sentence.
    pub sentence: String,
}

/// An exact fraction, kept in lowest terms with a positive denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rat {
    num: i128,
    den: i128,
}

fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

impl Rat {
    fn new(num: i128, den: i128) -> Option<Self> {
        if den == 0 {
            return None;
        }
        let sign = if den < 0 { -1 } else { 1 };
        let divisor = gcd(num, den).max(1);
        Some(Self {
            num: sign * num / divisor,
            den: sign * den / divisor,
        })
    }

    fn mul(self, other: Self) -> Option<Self> {
        Self::new(
            self.num.checked_mul(other.num)?,
            self.den.checked_mul(other.den)?,
        )
    }

    fn div(self, other: Self) -> Option<Self> {
        Self::new(
            self.num.checked_mul(other.den)?,
            self.den.checked_mul(other.num)?,
        )
    }

    fn add(self, other: Self) -> Option<Self> {
        Self::new(
            self.num
                .checked_mul(other.den)?
                .checked_add(other.num.checked_mul(self.den)?)?,
            self.den.checked_mul(other.den)?,
        )
    }

    fn neg(self) -> Self {
        Self {
            num: -self.num,
            den: self.den,
        }
    }

    fn is_whole(self) -> bool {
        self.den == 1
    }

    fn floor(self) -> i128 {
        self.num.div_euclid(self.den)
    }
}

/// Read `a/b`, `\frac{a}{b}`, a decimal or a whole number.
fn read(text: &str) -> Option<Rat> {
    let text = text.trim().trim_matches('$').trim();
    if let Some(rest) = text
        .strip_prefix("\\dfrac{")
        .or_else(|| text.strip_prefix("\\frac{"))
    {
        let (top, rest) = rest.split_once("}{")?;
        return Rat::new(
            top.trim().parse().ok()?,
            rest.strip_suffix('}')?.trim().parse().ok()?,
        );
    }
    if let Some((top, bottom)) = text.split_once('/') {
        return Rat::new(top.trim().parse().ok()?, bottom.trim().parse().ok()?);
    }
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 9 || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let scale = 10_i128.pow(u32::try_from(fraction.len()).ok()?);
    let negative = whole.starts_with('-');
    let whole: i128 = whole
        .parse()
        .ok()
        .or(if whole == "-" { Some(0) } else { None })?;
    let tail: i128 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().ok()?
    };
    let magnitude = whole.abs().checked_mul(scale)?.checked_add(tail)?;
    Rat::new(if negative { -magnitude } else { magnitude }, scale)
}

/// The first two fractions written in the problem, as `\frac{a}{b}` or `a/b`.
fn operands(problem: &str) -> Option<(Rat, Rat)> {
    let mut found = Vec::new();
    let mut rest = problem;
    while found.len() < 2 {
        let frac = ["\\dfrac{", "\\frac{"]
            .iter()
            .filter_map(|c| rest.find(c).map(|at| (at, *c)))
            .min_by_key(|(at, _)| *at);
        let Some((at, command)) = frac else { break };
        let body = &rest[at + command.len()..];
        let (top, tail) = body.split_once("}{")?;
        let (bottom, after) = tail.split_once('}')?;
        found.push(Rat::new(
            top.trim().parse().ok()?,
            bottom.trim().parse().ok()?,
        )?);
        rest = after;
    }
    match found.as_slice() {
        [a, b] => Some((*a, *b)),
        _ => None,
    }
}

fn push(list: &mut Vec<Candidate>, name: &'static str, sentence: &str) {
    if !list.iter().any(|c| c.name == name) {
        list.push(Candidate {
            name,
            sentence: sentence.to_owned(),
        });
    }
}

/// The mistakes that produce exactly `given` from the problem and its key.
#[must_use]
pub fn candidates(payload: &JobPayload) -> Vec<Candidate> {
    let (Some(key), Some(given)) = (read(&payload.expected), read(&payload.given_answer)) else {
        return Vec::new();
    };
    if key == given {
        return Vec::new();
    }
    let mut list = Vec::new();
    if let Some((a, b)) = operands(&payload.problem) {
        by_operands(&mut list, (a, b), key, given);
    }
    if !key.is_whole() && given.is_whole() {
        if given.num == key.floor() + 1 {
            push(
                &mut list,
                "rounded-up",
                "You rounded the answer up to a whole number.",
            );
        }
        if given.num == key.floor() {
            push(
                &mut list,
                "rounded-down",
                "You rounded the answer down to a whole number.",
            );
        }
    }
    if key.num != 0 && Rat::new(key.den, key.num) == Some(given) {
        push(
            &mut list,
            "reciprocal",
            "You gave the reciprocal of the answer.",
        );
    }
    if key.neg() == given {
        push(
            &mut list,
            "sign-slip",
            "You gave the answer with the opposite sign.",
        );
    }
    list
}

/// The wrong operation on the two fractions of the problem.
fn by_operands(list: &mut Vec<Candidate>, (a, b): (Rat, Rat), key: Rat, given: Rat) {
    let quotient = a.div(b);
    let product = a.mul(b);
    if quotient == Some(key) {
        if product == Some(given) {
            push(
                list,
                "multiplied-without-inverting",
                "You multiplied the two fractions and did not invert the second one.",
            );
        }
        if b.div(a) == Some(given) {
            push(
                list,
                "divided-wrong-way",
                "You divided the second fraction by the first.",
            );
        }
    }
    if product == Some(key) && quotient == Some(given) {
        push(
            list,
            "divided-instead-of-multiplied",
            "You divided the two fractions.",
        );
    }
    if a.add(b) == Some(given) {
        push(list, "added-the-fractions", "You added the two fractions.");
    }
    if a.add(b.neg()) == Some(given) {
        push(
            list,
            "subtracted-the-fractions",
            "You subtracted the two fractions.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::candidates;
    use cadus_store::diagnosis::JobPayload;

    fn payload(problem: &str, expected: &str, given: &str) -> JobPayload {
        JobPayload {
            v: 1,
            session: None,
            task_id: "t".to_owned(),
            topic: "dividing-fractions".to_owned(),
            kp: None,
            problem: problem.to_owned(),
            expected: expected.to_owned(),
            answer_kind: "number".to_owned(),
            given_answer: given.to_owned(),
            work: None,
            answer_property: None,
        }
    }

    fn names(problem: &str, expected: &str, given: &str) -> Vec<&'static str> {
        candidates(&payload(problem, expected, given))
            .iter()
            .map(|c| c.name)
            .collect()
    }

    const STEPS: &str = "$\\frac{3}{4}$ m is how many times as long as $\\frac{5}{8}$ m? \
                         Divide $\\frac{3}{4}$ by $\\frac{5}{8}$.";

    /// The fault of owner report 11: 2 for 6/5 is a rounding slip.
    #[test]
    fn two_for_six_fifths_is_a_round_up() {
        assert_eq!(names(STEPS, "6/5", "2"), vec!["rounded-up"]);
        assert_eq!(names(STEPS, "6/5", "1"), vec!["rounded-down"]);
    }

    /// The wrong operation on the two fractions is named by its result.
    #[test]
    fn the_wrong_operation_is_named_by_its_result() {
        assert_eq!(
            names(STEPS, "6/5", "15/32"),
            vec!["multiplied-without-inverting"]
        );
        assert!(names(STEPS, "6/5", "5/6").contains(&"divided-wrong-way"));
        assert!(names(STEPS, "6/5", "11/8").contains(&"added-the-fractions"));
        assert!(names(STEPS, "6/5", "1/8").contains(&"subtracted-the-fractions"));
    }

    /// A sign slip and an answer no table row explains.
    #[test]
    fn a_sign_slip_is_named_and_a_stray_answer_has_no_candidate() {
        assert_eq!(names("Compute 3 - 8.", "-5", "5"), vec!["sign-slip"]);
        assert!(names(STEPS, "6/5", "17").is_empty());
        assert!(names(STEPS, "6/5", "banana").is_empty());
    }
}
