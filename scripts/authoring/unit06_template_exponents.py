"""Exponent-rule recipes; variable-exponent refusals are retained as evidence."""
from fractions import Fraction


def recipes(add):
    """Declare each exponent objective with an independently calculated sample oracle."""
    product(add)
    powers(add)
    reciprocals(add)


def product(r):
    r("exponent-product-rule/kp1", "Simplify $x^{a}\\cdot x^3$.", "x**(a+3)",
      "The factors share base $x$: count {a} copies and three more, giving exponent ${a}+3$.",
      lambda a: f"x^{a+3}")
    r("exponent-product-rule/kp2", "Write $2^{a}\\cdot2^3$ as one power of $2$.", "2**(a+3)",
      "Combining the copies of base $2$ gives $2^{{{a}+3}}$.", lambda a: f"2^{a+3}")
    r("exponent-product-rule/kp3", "Simplify $x\\cdot x^{a}\\cdot x^2$.", "x**(a+3)",
      "The bare factor contributes one: $1+{a}+2={a}+3$, the exponent of $x$.",
      lambda a: f"x^{a+3}")
    r("exponent-quotient-rule/kp1", "Simplify $x^{{{a}+3}}/x^3$ for $x\\ne0$.", "x**a",
      "Cancel three factors of $x$: the exponent remaining is $({a}+3)-3={a}$.",
      lambda a: f"x^{a}")
    r("exponent-quotient-rule/kp2", "Write $3^{{{a}+2}}/3^2$ as one power of $3$.", "3**a",
      "The quotient exponent is $({a}+2)-2={a}$, giving $3^{{{a}}}$.", lambda a: f"3^{a}")
    r("exponent-quotient-rule/kp3", "Simplify $x^{a}/x$ for $x\\ne0$.", "x**(a-1)",
      "The denominator is $x^1$; cancellation leaves exponent ${a}-1$.", lambda a: f"x^{a-1}")
    r("exponent-product-quotient-rules/kp1", "Simplify $x^{a}x^4/x^2$ for $x\\ne0$.", "x**(a+2)",
      "Combine the numerator exponents, then subtract the denominator: ${a}+4-2={a}+2$.",
      lambda a: f"x^{a+2}")
    r("exponent-product-quotient-rules/kp2", "Simplify $(3x^2)({a}x^5)/(6x^3)$ for $x\\ne0$.", "(a/2)*x**4",
      "Coefficients give $3\\cdot{a}/6={a}/2$. The exponent is $2+5-3=4$, so the result is $({a}/2)x^4$.",
      lambda a: f"{Fraction(a,2)}*x^4")
    r("exponent-product-quotient-rules/kp3", "Simplify $(x^{a}y^2)(x^3y^4)/(xy)$ for nonzero $x,y$.",
      "x**(a+2)*y**5", "For $x$, ${a}+3-1={a}+2$; for $y$, $2+4-1=5$. Multiply the two resulting powers.",
      lambda a: f"x^{a+2}*y^5")


def powers(r):
    r("power-of-a-power-rule/kp1", "Simplify $(x^{a})^3$.", "x**(3*a)",
      "There are three groups of {a} factors, so the exponent is $3\\cdot{a}$.", lambda a: f"x^{3*a}")
    r("power-of-a-power-rule/kp2", "Write $(2^{a})^2$ as a single power of $2$.", "2**(2*a)",
      "Squaring repeats the whole group: ${a}+{a}=2\\cdot{a}$ copies of $2$.", lambda a: f"2^{2*a}")
    r("power-of-a-power-rule/kp3", "Simplify $(x^{a})^2/x^3$ for $x\\ne0$.", "x**(2*a-3)",
      "First multiply exponents to get $2\\cdot{a}$, then cancel three factors: exponent $2\\cdot{a}-3$.",
      lambda a: f"x^{2*a-3}")
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
    r("simplifying-negative-exponent-expressions/kp1", "Simplify $x^{{-3}}/x^{a}$ for $x\\ne0$.", "1/x**(a+3)",
      "The quotient exponent is $-3-{a}=-({a}+3)$, so write the reciprocal $1/x^{{{a}+3}}$.", lambda a: f"1/x^{a+3}")
    r("simplifying-negative-exponent-expressions/kp2", "Simplify $({a}x^{{-2}})^3$ for $x\\ne0$.", "a**3/x**6",
      "Cube the coefficient and multiply $-2$ by $3$: ${a}^3x^{{-6}}={a}^3/x^6$.", lambda a: f"{a**3}/x^6")
    r("simplifying-negative-exponent-expressions/kp3", "Simplify $({a}x^{{-2}}y^3)^2/(2xy^{{-1}})$ for nonzero $x,y$.",
      "(a**2/2)*y**7/x**5", "The numerator becomes ${a}^2x^{{-4}}y^6$. Division gives exponents $-4-1=-5$ and $6-(-1)=7$: ${a}^2y^7/(2x^5)$.",
      lambda a: f"{Fraction(a*a,2)}*y^7/x^5")
