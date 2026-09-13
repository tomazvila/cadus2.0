"""Exponent-rule recipes; variable-exponent refusals are retained as evidence."""
from fractions import Fraction


def recipes(add):
    """Declare each exponent objective with an independently calculated sample oracle."""
    product(add)
    powers(add)
    reciprocals(add)


def product(r):
    r("exponent-product-rule/kp1", "Simplify $({a}x^2)(3x^3)$.", "3*a*x**5",
      "The equal base $x$ gets exponent $2+3=5$; multiply the numerical coefficients to get $3\\cdot{a}$.",
      lambda a: f"{3*a}*x^5")
    r("exponent-product-rule/kp2", "Write ${a}^2\\cdot{a}^3$ as one power.", "powerform(1,[a,5])",
      "The factors have equal base ${a}$, so add exponents: $2+3=5$.", lambda a: f"({a})^(5)")
    r("exponent-product-rule/kp3", "Simplify $({a}x)(x^2)(2x^3)$.", "2*a*x**6",
      "Add the exponents $1+2+3=6$ and multiply the coefficients ${a}$ and $2$.",
      lambda a: f"{2*a}*x^6")
    r("exponent-quotient-rule/kp1", "Simplify $({a}x^7)/x^3$ for $x\\ne0$.", "a*x**4",
      "Subtract denominator exponent from numerator exponent: $7-3=4$; the coefficient ${a}$ remains.",
      lambda a: f"{a}*x^4")
    r("exponent-quotient-rule/kp2", "Write ${a}^7/{a}^3$ as one power.", "powerform(1,[a,4])",
      "The nonzero equal bases give exponent $7-3=4$.", lambda a: f"({a})^(4)")
    r("exponent-quotient-rule/kp3", "Simplify $({a}x^6)/x^2$ for $x\\ne0$.", "a*x**4",
      "Cancel two of the six factors of $x$, leaving exponent $6-2=4$ and coefficient ${a}$.", lambda a: f"{a}*x^4")
    r("exponent-product-quotient-rules/kp1", "Simplify $({a}x^3)(x^4)/x^2$ for $x\\ne0$.", "a*x**5",
      "Combine the numerator exponents and subtract the denominator exponent: $3+4-2=5$.",
      lambda a: f"{a}*x^5")
    r("exponent-product-quotient-rules/kp2", "Simplify $(3x^2)({a}x^5)/(6x^3)$ for $x\\ne0$.", "(a/2)*x**4",
      "Coefficients give $3\\cdot{a}/6={a}/2$. The exponent is $2+5-3=4$, so the result is $({a}/2)x^4$.",
      lambda a: f"{Fraction(a,2)}*x^4")
    r("exponent-product-quotient-rules/kp3", "Simplify $({a}x^4y^2)(x^3y^4)/(x^2y)$ for nonzero $x,y$.",
      "a*x**5*y**5", "For $x$, $4+3-2=5$; for $y$, $2+4-1=5$. Keep coefficient ${a}$.",
      lambda a: f"{a}*x^5*y^5")


def powers(r):
    r("power-of-a-power-rule/kp1", "Simplify $({a}x^2)^3$.", "a**3*x**6",
      "Cube the coefficient and multiply the exponents: $(x^2)^3=x^6$.", lambda a: f"{a**3}*x^6",
      list(range(3, 15)))
    r("power-of-a-power-rule/kp2", "Write $({a}^2)^3$ as one power.", "powerform(1,[a,6])",
      "Multiply the inner and outer exponents: $2\\cdot3=6$.", lambda a: f"({a})^(6)")
    r("power-of-a-power-rule/kp3", "Simplify $({a}x^4)^2/x^3$ for $x\\ne0$.", "a**2*x**5",
      "Square the coefficient, multiply $4\\cdot2=8$, then subtract the denominator exponent: $8-3=5$.",
      lambda a: f"{a*a}*x^5")
    r("power-rule-exponents/kp1", "Expand the power $({a}x^3)^2$.", "a**2*x**6",
      "Square both factors: $({a}\\cdot{a})(x^3x^3)={a}^2x^6$.", lambda a: f"{a*a}*x^6")
    r("power-rule-exponents/kp2", "Simplify $({a}/x^2)^3$ for $x\\ne0$.", "a**3/x**6",
      "Cube numerator and denominator: ${a}^3/(x^2)^3={a}^3/x^6$.", lambda a: f"{a**3}/x^6")
    r("power-rule-exponents/kp3", "Simplify $({a}x^2)^2x^3/(2x)$ for $x\\ne0$.", "(a**2/2)*x**6",
      "Squaring gives ${a}^2x^4$. Then $x$ has exponent $4+3-1=6$ and the coefficient is ${a}^2/2$.",
      lambda a: f"{Fraction(a*a,2)}*x^6")
    r("zero-exponent-rule/kp1", "Evaluate $(-{a})^0$.", "1",
      "The base $-{a}$ is nonzero, so its zero power is the multiplicative identity $1$.", lambda a: "1")
    r("zero-exponent-rule/kp2", "Evaluate ${a}+7^0-3^0$.", "a",
      "Both nonzero bases have zero power $1$, so ${a}+1-1={a}$.", lambda a: str(a))
    r("zero-exponent-rule/kp3", "Evaluate ${a}^4/{a}^4$ and express its exponent as a difference.", "1",
      "Equal nonzero quantities divide to $1$. The quotient rule gives ${a}^{{4-4}}={a}^0$, explaining the same value.",
      lambda a: "1")


def reciprocals(r):
    r("negative-zero-exponents/kp1", "Evaluate ${a}^{{-2}}$ as an exact fraction.", "1/a**2",
      "The negative exponent gives a reciprocal: $1/{a}^2=1/({a}\\cdot{a})$.", lambda a: str(Fraction(1,a*a)))
    r("negative-zero-exponents/kp2", "Simplify $1/({a}x^{{-3}})$ for $x\\ne0$.", "x**3/a",
      "Replace $x^{{-3}}$ with $1/x^3$; dividing by ${a}/x^3$ gives $x^3/{a}$.", lambda a: f"x^3/{a}")
    r("negative-zero-exponents/kp3", "Simplify ${a}x^{{-2}}x^5$ for $x\\ne0$.", "a*x**3",
      "The exponents combine to $-2+5=3$ while the coefficient stays ${a}$, giving ${a}x^3$.", lambda a: f"{a}*x^3")
    r("simplifying-negative-exponent-expressions/kp1", "Simplify $({a}x^{{-3}})/x^2$ for $x\\ne0$.", "a/x**5",
      "The quotient exponent is $-3-2=-5$, so move $x^5$ to the denominator and keep coefficient ${a}$.", lambda a: f"{a}/x^5")
    r("simplifying-negative-exponent-expressions/kp2", "Simplify $({a}x^{{-2}})^3$ for $x\\ne0$.", "a**3/x**6",
      "Cube the coefficient and multiply $-2$ by $3$: ${a}^3x^{{-6}}={a}^3/x^6$.", lambda a: f"{a**3}/x^6")
    r("simplifying-negative-exponent-expressions/kp3", "Simplify $({a}x^{{-2}}y^3)^2/(2xy^{{-1}})$ for nonzero $x,y$.",
      "(a**2/2)*y**7/x**5", "The numerator becomes ${a}^2x^{{-4}}y^6$. Division gives exponents $-4-1=-5$ and $6-(-1)=7$: ${a}^2y^7/(2x^5)$.",
      lambda a: f"{Fraction(a*a,2)}*y^7/x^5")
