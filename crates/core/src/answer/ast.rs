//! The decidable answer AST (V1, D6).
//!
//! The tree holds exact values only. An integer is a `BigInt`. A decimal keeps its
//! mantissa and its scale, so `0.7` stays `7 / 10^1` and never becomes a float.
//! Unit-carrying answers are out of scope (spec section 8.3 residue).

use num_bigint::BigInt;

/// A named mathematical constant of the grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Const {
    /// The ratio of a circle's circumference to its diameter.
    Pi,
    /// Euler's number.
    E,
}

impl Const {
    /// Return the canonical spelling of the constant.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::E => "e",
        }
    }
}

/// The comparison operator of a simple inequality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IneqOp {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

impl IneqOp {
    /// Return the operator that holds when the two sides change place.
    ///
    /// `4 < x` and `x > 4` are the same predicate, so the parser puts the variable
    /// on the left and flips the operator with this function.
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::Lt => Self::Gt,
            Self::Le => Self::Ge,
            Self::Gt => Self::Lt,
            Self::Ge => Self::Le,
        }
    }

    /// Return the ASCII spelling of the operator.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }
}

/// One node of the decidable answer grammar (spec section 8.1 plus the interval
/// production).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ast {
    /// An arbitrary-precision integer.
    Integer(BigInt),
    /// An exact decimal: `mantissa / 10^scale`. `0.70` is `70 / 10^2`.
    Decimal {
        /// The digits with the sign, and without the point.
        mantissa: BigInt,
        /// The count of digits after the point.
        scale: u32,
    },
    /// A literal fraction of two integers. The denominator is never zero.
    Fraction {
        /// The numerator, with the sign of the fraction.
        numerator: BigInt,
        /// The denominator. Always positive and never zero.
        denominator: BigInt,
    },
    /// A mixed number `a b/c`, which reads as `sign(a) * (|a| + b/c)`.
    Mixed {
        /// The whole part. Its sign is the sign of the whole value.
        whole: BigInt,
        /// The numerator of the fractional part. Never negative.
        numerator: BigInt,
        /// The denominator of the fractional part. Always positive.
        denominator: BigInt,
    },
    /// A variable. A single letter, or one of the spelled Greek names.
    Var(String),
    /// A named constant.
    Const(Const),
    /// A square root.
    Sqrt(Box<Ast>),
    /// A power with an integer exponent. The grammar allows no other exponent.
    Pow(Box<Ast>, i64),
    /// Arithmetic negation.
    Neg(Box<Ast>),
    /// A sum of two or more terms.
    Add(Vec<Ast>),
    /// A product of two or more factors.
    Mul(Vec<Ast>),
    /// A quotient. The parser folds an integer over an integer into `Fraction`.
    Div(Box<Ast>, Box<Ast>),
    /// An application of a whitelisted function.
    Func(String, Vec<Ast>),
    /// An ordered tuple, written `(a, b)` or bare as `a, b`.
    Tuple(Vec<Ast>),
    /// An unordered set, written `{a, b}`.
    Set(Vec<Ast>),
    /// An ordered list, written `[a, b]`.
    List(Vec<Ast>),
    /// An interval with an explicit end style, written `(a, b]` or `[a, b)`.
    Interval {
        /// The lower end.
        lo: Box<Ast>,
        /// The upper end.
        hi: Box<Ast>,
        /// True when the lower end belongs to the interval.
        lo_closed: bool,
        /// True when the upper end belongs to the interval.
        hi_closed: bool,
    },
    /// A simple inequality over one variable, always with the variable on the left.
    Ineq {
        /// The variable name.
        var: String,
        /// The comparison operator.
        op: IneqOp,
        /// The bound.
        bound: Box<Ast>,
    },
    /// A chained inequality `lo <= var <= hi`, with the end style of each side.
    Chain {
        /// The lower bound.
        lo: Box<Ast>,
        /// True when the lower comparison allows equality.
        lo_closed: bool,
        /// The variable name.
        var: String,
        /// True when the upper comparison allows equality.
        hi_closed: bool,
        /// The upper bound.
        hi: Box<Ast>,
    },
}
