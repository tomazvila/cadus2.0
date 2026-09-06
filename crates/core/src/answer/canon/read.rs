//! The reader of the canonicalizer: one tree node into one canonical value.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use super::sum::{atom_value, extract_square, from_sum, term};
use super::{Atom, Canon, MAX_SCALE, Monomial, Undecidable, Work};
use crate::answer::ast::{Ast, Const, IneqOp};
use crate::answer::unit::lookup;

impl Work {
    /// Canonicalize one node by its kind.
    pub(super) fn dispatch(&mut self, ast: &Ast) -> Result<Canon, Undecidable> {
        match ast {
            Ast::Integer(value) => self.rational(BigRational::from_integer(value.clone())),
            Ast::Decimal { mantissa, scale } => self.decimal(mantissa, *scale),
            Ast::Fraction {
                numerator,
                denominator,
            } => self.exact_fraction(numerator, denominator),
            Ast::Mixed {
                whole,
                numerator,
                denominator,
            } => self.mixed(whole, numerator, denominator),
            Ast::Var(name) => Ok(atom_value(Atom::Var(name.clone()))),
            Ast::Const(constant) => Ok(constant_value(*constant)),
            Ast::Sqrt(inner) => self.root(inner),
            Ast::Pow(base, exponent) => self.raised(base, *exponent),
            Ast::RationalPow {
                base,
                numerator,
                denominator,
            } => self.rational_exponent_power(base, *numerator, *denominator),
            Ast::Neg(inner) => self.negated(inner),
            Ast::Add(items) | Ast::Mul(items) => self.fold(ast, items),
            Ast::Div(dividend, divisor) => self.ratio(dividend, divisor),
            Ast::Func(name, arguments) => self.applied(name, arguments),
            Ast::Tuple(items) | Ast::List(items) | Ast::Set(items) => self.collection(ast, items),
            Ast::Assign { var, value } => self.labeled(var, value),
            Ast::Quantity { value, unit } => self.quantity(value, unit),
            Ast::Interval {
                lo,
                hi,
                lo_closed,
                hi_closed,
            } => self.interval(None, lo, *lo_closed, hi, *hi_closed),
            Ast::Ineq { var, op, bound } => self.half_line(var, *op, bound),
            Ast::Chain {
                lo,
                lo_closed,
                var,
                hi_closed,
                hi,
            } => self.interval(Some(var.clone()), lo, *lo_closed, hi, *hi_closed),
        }
    }

    /// Read the root of the grammar.
    ///
    /// `\sqrt{a}`, `√a`, and `sqrt(a)` all build this one node, so the three
    /// spellings take one path (FIXM2g).
    fn root(&mut self, inner: &Ast) -> Result<Canon, Undecidable> {
        let argument = self.node(inner)?;
        self.call("sqrt", vec![argument])
    }

    /// Read a power with an integer exponent.
    fn raised(&mut self, base: &Ast, exponent: i64) -> Result<Canon, Undecidable> {
        let base = self.node(base)?;
        self.power(&base, exponent)
    }

    /// Read a negation as a product with minus one.
    fn negated(&mut self, inner: &Ast) -> Result<Canon, Undecidable> {
        let value = self.node(inner)?;
        let minus_one = Canon::Rational(-BigRational::one());
        self.multiply(&minus_one, &value)
    }

    /// Read a sum or a product, by the kind of the node.
    fn fold(&mut self, ast: &Ast, items: &[Ast]) -> Result<Canon, Undecidable> {
        if matches!(ast, Ast::Add(_)) {
            self.sum(items)
        } else {
            self.product(items)
        }
    }

    /// Read a sum of terms, from zero upward.
    fn sum(&mut self, items: &[Ast]) -> Result<Canon, Undecidable> {
        let mut total = Canon::Rational(BigRational::zero());
        for item in items {
            let value = self.node(item)?;
            total = self.add(&total, &value)?;
        }
        Ok(total)
    }

    /// Read a product of factors, from one upward.
    fn product(&mut self, items: &[Ast]) -> Result<Canon, Undecidable> {
        let mut product = Canon::Rational(BigRational::one());
        for item in items {
            let value = self.node(item)?;
            product = self.multiply(&product, &value)?;
        }
        Ok(product)
    }

    /// Read a quotient.
    ///
    /// Division and the `\frac{a}{b}` construct are one operation: the parser
    /// builds one [`Ast::Div`] node for both, and `Ast::Fraction` for two integer
    /// bodies (FIXM2g). A percent is a division by the literal 100 at the parser.
    fn ratio(&mut self, dividend: &Ast, divisor: &Ast) -> Result<Canon, Undecidable> {
        let dividend = self.node(dividend)?;
        let divisor = self.node(divisor)?;
        self.divide(&dividend, &divisor)
    }

    /// Read a function application.
    fn applied(&mut self, name: &str, arguments: &[Ast]) -> Result<Canon, Undecidable> {
        let values = self.items(arguments)?;
        self.call(name, values)
    }

    /// Read a tuple, a list, or a set, by the kind of the node.
    ///
    /// A set is unordered, and its repeated members collapse into one member.
    fn collection(&mut self, ast: &Ast, items: &[Ast]) -> Result<Canon, Undecidable> {
        let values = self.items(items)?;
        Ok(match ast {
            Ast::Tuple(_) => Canon::Tuple(values),
            Ast::List(_) => Canon::List(values),
            _ => Canon::Set(values.into_iter().collect()),
        })
    }

    /// Read a number with a unit into the base unit of its kind (D-F3).
    ///
    /// The value must be a number: a rational or a radical. A unit outside the
    /// table never reaches this function from the parser, and a hand-built tree
    /// that carries one is refused.
    fn quantity(&mut self, value: &Ast, unit: &str) -> Result<Canon, Undecidable> {
        let value = self.node(value)?;
        if !matches!(value, Canon::Rational(_) | Canon::Radical(_)) {
            return Err(Undecidable::new("a quantity whose value is not a number"));
        }
        let Some(unit) = lookup(unit) else {
            return Err(Undecidable::new("a unit outside the table"));
        };
        let scaled = self.multiply(&value, &Canon::Rational(unit.factor()))?;
        Ok(Canon::Quantity {
            quantity: unit.quantity,
            value: Box::new(scaled),
        })
    }

    /// Read a labeled value.
    ///
    /// The label is not part of the value, so the canonical form keeps it apart.
    /// `check` is the step that compares the two labels (FIXM2b, review findings
    /// #2, #10, #16).
    fn labeled(&mut self, var: &str, value: &Ast) -> Result<Canon, Undecidable> {
        let value = self.node(value)?;
        Ok(Canon::Assign {
            var: var.to_string(),
            value: Box::new(value),
        })
    }

    /// Read a range with two ends: a bracket pair, or a chained inequality.
    fn interval(
        &mut self,
        var: Option<String>,
        lo: &Ast,
        lo_closed: bool,
        hi: &Ast,
        hi_closed: bool,
    ) -> Result<Canon, Undecidable> {
        let lo = self.node(lo)?;
        let hi = self.node(hi)?;
        Ok(Canon::Interval {
            var,
            lo: Some(Box::new(lo)),
            lo_closed,
            hi: Some(Box::new(hi)),
            hi_closed,
        })
    }

    /// Read a simple inequality as a range with one end.
    fn half_line(&mut self, var: &str, op: IneqOp, bound: &Ast) -> Result<Canon, Undecidable> {
        let bound = Box::new(self.node(bound)?);
        Ok(match op {
            IneqOp::Lt | IneqOp::Le => Canon::Interval {
                var: Some(var.to_string()),
                lo: None,
                lo_closed: false,
                hi: Some(bound),
                hi_closed: op == IneqOp::Le,
            },
            IneqOp::Gt | IneqOp::Ge => Canon::Interval {
                var: Some(var.to_string()),
                lo: Some(bound),
                lo_closed: op == IneqOp::Ge,
                hi: None,
                hi_closed: false,
            },
        })
    }

    /// Canonicalize every item of a collection.
    fn items(&mut self, items: &[Ast]) -> Result<Vec<Canon>, Undecidable> {
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(self.node(item)?);
        }
        Ok(values)
    }

    /// Read an exact decimal as `mantissa / 10^scale`.
    fn decimal(&mut self, mantissa: &BigInt, scale: u32) -> Result<Canon, Undecidable> {
        if scale > MAX_SCALE {
            return Err(Undecidable::new("a decimal past the size bound"));
        }
        self.spend(1)?;
        let denominator = BigInt::from(10_u32).pow(scale);
        self.rational(BigRational::new(mantissa.clone(), denominator))
    }

    /// Read a mixed number `a b/c` as `sign(a) * (|a| + b/c)`.
    fn mixed(
        &mut self,
        whole: &BigInt,
        numerator: &BigInt,
        denominator: &BigInt,
    ) -> Result<Canon, Undecidable> {
        if denominator.is_zero() {
            return Err(Undecidable::new("a division by zero"));
        }
        self.spend(1)?;
        let magnitude = BigRational::from_integer(whole.abs())
            + BigRational::new(numerator.clone(), denominator.clone());
        let value = if whole.is_negative() {
            -magnitude
        } else {
            magnitude
        };
        self.rational(value)
    }

    /// Apply a whitelisted function to canonical arguments.
    fn call(&mut self, name: &str, arguments: Vec<Canon>) -> Result<Canon, Undecidable> {
        self.spend(1)?;
        // `ln` and `log` are one function: the natural logarithm. 1.0 makes `ln`
        // an alias of `log`, and the corpus authors both spellings on the topic
        // `change-of-base-formula`.
        let name = if name == "ln" { "log" } else { name };
        if arguments.len() == 1 {
            if name == "sqrt"
                && let Some(argument) = arguments.first()
            {
                // `sqrt(x)` and `x^(1/2)` are one value, so the root of a value
                // that is no rational takes the root law (D-F3).
                let Canon::Rational(value) = argument else {
                    let argument = argument.clone();
                    return self.root_power(&argument, 1, 2);
                };
                let value = value.clone();
                return self.root_of_rational(&value, arguments);
            }
            if name == "exp"
                && let Some(argument) = arguments.first()
            {
                let argument = argument.clone();
                return self.exponential(&argument);
            }
        }
        Ok(atom_value(Atom::Call(name.to_string(), arguments)))
    }

    /// Read `exp(a)` into the canonical exponential.
    ///
    /// A whole `a` gives the atom `e` with exponent `a`, so `exp(2)` and `e**2`
    /// are one value. Every other `a` gives [`Atom::Exp`].
    fn exponential(&mut self, argument: &Canon) -> Result<Canon, Undecidable> {
        let mut monomial = Monomial::new();
        self.add_exp(&mut monomial, argument, 1)?;
        Ok(from_sum(term(monomial, BigRational::one())))
    }

    /// Reduce `sqrt(p/q)` into `sqrt(p*q)/q` (M2 review 3, findings 9 and 11).
    ///
    /// `p/q` is in lowest terms with a positive `q`, so `p*q` is a whole number
    /// and the identity is exact. `sqrt(1/2)` therefore becomes `sqrt(2)/2` and
    /// `sqrt(4/9)` becomes `2/3`, which are the two forms 1.0 answers True for.
    pub(super) fn root_of_rational(
        &mut self,
        value: &BigRational,
        arguments: Vec<Canon>,
    ) -> Result<Canon, Undecidable> {
        if value.is_negative() {
            // The grammar holds real values only, so a negative radicand keeps its
            // function application and compares structurally.
            return Ok(atom_value(Atom::Call("sqrt".to_string(), arguments)));
        }
        if value.is_zero() {
            return Ok(Canon::Rational(BigRational::zero()));
        }
        let denominator = value.denom().clone();
        let radicand = value.numer() * &denominator;
        self.bounded_int(&radicand)?;
        let root = self.root_of_integer(&radicand)?;
        if denominator.is_one() {
            return Ok(root);
        }
        let scale = BigRational::new(BigInt::one(), denominator);
        let scale = Canon::Rational(self.bounded(scale)?);
        self.multiply(&root, &scale)
    }

    /// Reduce `sqrt(n)` for a positive whole number `n` into `outside * sqrt(radicand)`.
    fn root_of_integer(&mut self, value: &BigInt) -> Result<Canon, Undecidable> {
        self.spend(1)?;
        let (outside, radicand) = extract_square(value)?;
        let mut monomial = Monomial::new();
        let mut coefficient = self.bounded(BigRational::from_integer(outside))?;
        if !radicand.is_one() {
            self.add_atom(&mut monomial, &mut coefficient, &Atom::Sqrt(radicand), 1)?;
        }
        Ok(from_sum(term(monomial, coefficient)))
    }
}

/// Build the canonical value of a named constant.
fn constant_value(constant: Const) -> Canon {
    match constant {
        Const::Pi => atom_value(Atom::Pi),
        Const::E => atom_value(Atom::E),
    }
}
