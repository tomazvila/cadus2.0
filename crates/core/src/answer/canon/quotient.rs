//! The rational-function form: one numerator sum over one denominator sum.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use super::sum::{from_frac, is_one, one_poly, one_term, reciprocal_of, term};
use super::{Atom, Canon, Frac, Monomial, Poly, Undecidable, Work};

impl Work {
    /// Split a sum into its rational content and its primitive part.
    ///
    /// The primitive part has coprime integer coefficients, and its greatest
    /// monomial carries a positive coefficient. The content holds the sign.
    /// `2*x + 2` therefore becomes `2` and `x + 1`, so `1/(x+1)` and `2/(2*x+2)`
    /// are one value.
    ///
    /// The fold runs the size bound and the width charge after every step. The
    /// least common multiple of many coprime denominators grows fast, and an
    /// unbounded fold cost one check 8.4 s of CPU (M2 review 1, finding 5). The
    /// width charge on the coefficients already empties the budget before a
    /// bomb of that shape reaches this function, so the two bounds hold the same
    /// case twice.
    fn content_normalize(&mut self, sum: &Poly) -> Result<(BigRational, Poly), Undecidable> {
        let mut numerator_gcd = BigInt::zero();
        let mut denominator_lcm = BigInt::one();
        for coefficient in sum.values() {
            self.spend(1)?;
            numerator_gcd = numerator_gcd.gcd(coefficient.numer());
            denominator_lcm = denominator_lcm.lcm(coefficient.denom());
            self.bounded_int(&numerator_gcd)?;
            self.bounded_int(&denominator_lcm)?;
        }
        let leading_is_negative = sum
            .iter()
            .next_back()
            .is_some_and(|(_, coefficient)| coefficient.is_negative());
        let magnitude = BigRational::new(numerator_gcd, denominator_lcm);
        let signed = if leading_is_negative {
            -magnitude
        } else {
            magnitude
        };
        let content = self.bounded(signed)?;
        let divisor = reciprocal_of(&content);
        let mut primitive = Poly::new();
        // No coefficient is zero and the divisor is not zero, so no scaled
        // coefficient is zero.
        for (monomial, coefficient) in sum {
            let scaled = self.bounded(coefficient * &divisor)?;
            primitive.insert(monomial.clone(), scaled);
        }
        Ok((content, primitive))
    }

    /// Build the canonical value of one numerator over one denominator.
    ///
    /// The rules of [`Work::quotient`] read every term of the two sums, and the
    /// demotion of [`from_frac`] reads every term of the numerator again, so the
    /// step charges both sums (M2 review 4, finding 2).
    pub(super) fn value(&mut self, num: Poly, den: Poly) -> Result<Canon, Undecidable> {
        self.rebuild(&num)?;
        self.rebuild(&den)?;
        let quotient = self.quotient(num, den)?;
        Ok(from_frac(quotient))
    }

    /// Put one numerator over one denominator into the rational-function form.
    ///
    /// The rules of the module header run here, in order: a denominator of one
    /// term folds into the numerator as negative exponents; every negative
    /// exponent of a true quotient clears into the denominator; and the
    /// denominator is content- and sign-normalized with the scale moved into the
    /// numerator. The step cancels no polynomial common factor.
    ///
    /// # Errors
    ///
    /// Returns [`Undecidable`] for a zero denominator, and for a value past the
    /// work, term, or size bound.
    fn quotient(&mut self, num: Poly, den: Poly) -> Result<Frac, Undecidable> {
        if den.is_empty() {
            return Err(Undecidable::new("a division by zero"));
        }
        if num.is_empty() {
            return Ok(Frac {
                num,
                den: one_poly(),
            });
        }
        if is_one(&den) {
            return Ok(Frac { num, den });
        }
        // Rule 3. A true quotient carries no common monomial factor and no
        // negative exponent. `1/x * 1/(x+h)` therefore reaches the form of
        // `1/(x*(x+h))` (M2 review 3, finding 10). The step covers rule 2 as
        // well: a denominator of one term divides itself away, and the fold
        // below finishes it.
        let common = common_monomial(&num, &den);
        let (num, den) = if common.is_empty() {
            (num, den)
        } else {
            self.spend(common.len())?;
            let (monomial, coefficient) = self.power_of_term(&common, &BigRational::one(), -1)?;
            let factor = term(monomial, coefficient);
            let num = self.poly_mul(&num, &factor)?;
            let den = self.poly_mul(&den, &factor)?;
            (num, den)
        };
        if let Some((monomial, coefficient)) = one_term(&den) {
            // Rule 2. A denominator of one term is negative exponents of the
            // numerator, so `1/x` stays the monomial `x**-1`. A product of two
            // sums of two terms or more reaches this line too, when two atoms
            // merge: `(sqrt(2)+1)*(sqrt(2)-1)` is 1.
            return self.fold_monomial_denominator(num, &monomial, &coefficient);
        }
        // Rule 4. The denominator holds coprime integer coefficients and a
        // positive greatest monomial; the scale moves into the numerator.
        let (content, primitive) = self.content_normalize(&den)?;
        let scale = reciprocal_of(&content);
        let scale = self.bounded(scale)?;
        let num = self.poly_mul(&num, &term(Monomial::new(), scale))?;
        // Rule 5. A numerator that is a rational multiple of the denominator is
        // that rational: `(x+1)/(x+1)` is 1 and `(2*x+2)/(x+1)` is 2. The test
        // compares the two monomial keys first and the two coefficient ratios
        // after, so it costs no division of one polynomial by another.
        if let Some(scalar) = self.scalar_ratio(&num, &primitive)? {
            return Ok(Frac {
                num: term(Monomial::new(), scalar),
                den: one_poly(),
            });
        }
        Ok(Frac {
            num,
            den: primitive,
        })
    }

    /// Read the rational `r` of `num = r * den`, when there is one.
    ///
    /// The two sums must hold the same monomials, and every coefficient of the
    /// one must be the same multiple of the coefficient of the other. The test
    /// runs no polynomial division: it is the numerator half of the content
    /// normalization of rule 4.
    fn scalar_ratio(&mut self, num: &Poly, den: &Poly) -> Result<Option<BigRational>, Undecidable> {
        if num.len() != den.len() || !num.keys().eq(den.keys()) {
            return Ok(None);
        }
        let mut ratio: Option<BigRational> = None;
        for (left, right) in num.values().zip(den.values()) {
            self.spend(1)?;
            match &ratio {
                None => ratio = Some(self.bounded(left / right)?),
                Some(present) => {
                    let scaled = self.bounded(present * right)?;
                    if &scaled != left {
                        return Ok(None);
                    }
                }
            }
        }
        Ok(ratio)
    }

    /// Fold a denominator of one term into the numerator as negative exponents.
    ///
    /// Rule 2 of the module header. `1/x` therefore stays the monomial `x**-1`,
    /// and `1/sqrt(2)` stays `sqrt(2)/2`, because the reciprocal of the term goes
    /// through the same atom laws as every other product.
    fn fold_monomial_denominator(
        &mut self,
        num: Poly,
        monomial: &Monomial,
        coefficient: &BigRational,
    ) -> Result<Frac, Undecidable> {
        self.spend(monomial.len().saturating_add(1))?;
        let (monomial, coefficient) = self.power_of_term(monomial, coefficient, -1)?;
        let num = self.poly_mul(&num, &term(monomial, coefficient))?;
        Ok(Frac {
            num,
            den: one_poly(),
        })
    }

    /// Promote a canonical form back into the internal quotient of two sums.
    ///
    /// # Errors
    ///
    /// Returns [`Undecidable`] for a collection: a tuple, a set, a list, a range,
    /// and a labeled value carry no arithmetic.
    pub(super) fn frac_of(&mut self, value: &Canon) -> Result<Frac, Undecidable> {
        let num = match value {
            Canon::Rational(number) => term(Monomial::new(), number.clone()),
            Canon::Radical(parts) => {
                // The promotion touches every part, so it charges every part
                // (M2 review 4, finding 2).
                self.spend(parts.len())?;
                let mut sum = Poly::new();
                for (basis, coefficient) in parts {
                    let mut monomial = Monomial::new();
                    if !basis.radicand.is_one() {
                        monomial.insert(Atom::Sqrt(basis.radicand.clone()), 1);
                    }
                    if basis.pi != 0 {
                        monomial.insert(Atom::Pi, basis.pi);
                    }
                    if basis.e != 0 {
                        monomial.insert(Atom::E, basis.e);
                    }
                    self.insert_term(&mut sum, monomial, coefficient.clone())?;
                }
                sum
            }
            Canon::Poly(parts) => {
                self.rebuild(parts)?;
                parts.clone()
            }
            Canon::Value { num, den } => {
                self.rebuild(num)?;
                self.rebuild(den)?;
                return Ok(Frac {
                    num: num.clone(),
                    den: den.clone(),
                });
            }
            Canon::Func(name, arguments) => {
                let mut monomial = Monomial::new();
                monomial.insert(Atom::Call(name.clone(), arguments.clone()), 1);
                term(monomial, BigRational::one())
            }
            Canon::Tuple(_) | Canon::Set(_) | Canon::List(_) | Canon::Interval { .. } => {
                return Err(Undecidable::new("arithmetic on a collection"));
            }
            Canon::Assign { .. } => {
                return Err(Undecidable::new("arithmetic on a labeled value"));
            }
        };
        Ok(Frac {
            num,
            den: one_poly(),
        })
    }
}

/// Build the greatest monomial that divides every term of two sums.
///
/// The monomial is the monomial content of the quotient, and it carries each
/// atom to the LEAST power any term of the two sums gives it. A term that holds
/// no such atom gives it the power 0, so the result carries a positive power
/// only when every term of both sums holds that atom.
///
/// The monomial content is the reason the form is canonical. Rule 3 without it
/// clears the negative exponents but keeps a common factor: `4/s - 5/s**2 +
/// 2/(s-3)` reaches the denominator `s**3-3*s**2` and the same value written
/// `(2*s**3 + 4*s**2*(s-3) - 5*s*(s-3))/(s**3*(s-3))` reaches `s**4-3*s**3`. The
/// content takes the factor `s` out of the second one and the two meet.
///
/// The step takes a MONOMIAL content only. It runs no polynomial division and it
/// takes no polynomial factor, so `(x**2-1)/(x-1)` keeps its denominator.
fn common_monomial(num: &Poly, den: &Poly) -> Monomial {
    let mut common = least_of([num, den]);
    // [`Atom::Sqrt`] and [`Atom::Exp`] never carry a negative exponent:
    // `1/sqrt(2)` is `sqrt(2)/2` and `1/e**x` is `e**(-x)`. A numerator therefore
    // cannot hold one of them back, and the content of the denominator alone is
    // the right content for those two atoms. Without this line
    // `1/(sqrt(2)*(x+1))` and `1/sqrt(2) * 1/(x+1)` are two forms of one value.
    for (atom, exponent) in least_of([den]) {
        if exponent > 0 && matches!(atom, Atom::Sqrt(_) | Atom::Exp(_)) {
            common.insert(atom, exponent);
        }
    }
    common
}

/// Build the monomial of the least power of each atom of a group of sums.
fn least_of<'a, I: IntoIterator<Item = &'a Poly>>(sums: I) -> Monomial {
    let mut least: Option<Monomial> = None;
    for sum in sums {
        for monomial in sum.keys() {
            least = Some(match least {
                None => monomial.clone(),
                Some(present) => least_powers(&present, monomial),
            });
        }
    }
    least.unwrap_or_default()
}

/// Build the monomial of the least power of each atom of two monomials.
///
/// An atom that one monomial does not hold has the power 0 there, so it survives
/// only with a negative power. A power of 0 leaves the result.
fn least_powers(left: &Monomial, right: &Monomial) -> Monomial {
    let mut least = Monomial::new();
    for (atom, exponent) in left {
        let other = right.get(atom).copied().unwrap_or(0);
        let value = (*exponent).min(other);
        if value != 0 {
            least.insert(atom.clone(), value);
        }
    }
    for (atom, exponent) in right {
        if left.contains_key(atom) {
            continue;
        }
        if *exponent < 0 {
            least.insert(atom.clone(), *exponent);
        }
    }
    least
}
