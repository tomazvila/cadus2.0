//! The decidable answer AST (V1, D6).
//!
//! The tree holds exact values only. An integer is a `BigInt`. A decimal keeps its
//! mantissa and its scale, so `0.7` stays `7 / 10^1` and never becomes a float.

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
    /// A mixed number `a b/c`, which reads as `a + b/c`.
    ///
    /// The whole part is the magnitude, and it is never negative. The parser
    /// wraps a negative mixed number in [`Ast::Neg`], because the sign belongs
    /// to the sign token and not to the integer value: `-0` is the integer zero,
    /// and a whole part that carries the sign drops the minus of `-0 1/2`
    /// (review round 3, finding #7).
    Mixed {
        /// The magnitude of the whole part. Never negative.
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
    ///
    /// Every root builds this node: the name `sqrt`, `\sqrt{a}`, `\sqrt a`, and
    /// the glyph `√` (review round 3, the structural ruling).
    Sqrt(Box<Ast>),
    /// A power with an integer exponent.
    Pow(Box<Ast>, i64),
    /// A power with a rational exponent `p/q`, which reads into a root (D-F3).
    ///
    /// The parser reduces the exponent to lowest terms and builds [`Ast::Pow`]
    /// for a whole exponent, so the denominator here is 2 or more. `2^(1/2)` is
    /// `sqrt(2)`, `8^(2/3)` is 4, and `x^(1/2)` is `sqrt(x)`.
    RationalPow {
        /// The base.
        base: Box<Ast>,
        /// The numerator of the exponent, with its sign.
        numerator: i64,
        /// The denominator of the exponent. Always 2 or more.
        denominator: i64,
    },
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
    /// A number with a unit of the Foundations table, written `5 cm` (D-F3).
    ///
    /// The unit is the table spelling. The canonicalizer scales the value into
    /// the base unit of its kind, so `1 m` and `100 cm` are one value.
    Quantity {
        /// The value, as the answer writes it.
        value: Box<Ast>,
        /// The unit spelling of [`crate::answer::unit`].
        unit: &'static str,
    },
    /// A value with its label, written `x = 5` (review findings #2, #10, #16).
    ///
    /// The label is the name the answer gives its value. It never reaches the
    /// value itself: `check` compares the two labels, and `canon` reads the value
    /// alone. 1.0 deleted the label, which made `x = 4` and `y = 4` one answer.
    Assign {
        /// The variable name, with its case as the answer writes it.
        var: String,
        /// The value the label names.
        value: Box<Ast>,
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
