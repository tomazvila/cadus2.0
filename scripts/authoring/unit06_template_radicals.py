"""Radical algebra and geometry recipes with exact independent sample answers."""
from fractions import Fraction
from math import isqrt


def recipes(r):
    simplification(r)
    operations(r)
    advanced(r)


def simplification(r):
    r("simplifying-radicals/kp1", "Simplify $\\sqrt{{{a}}}$ by extracting the largest square factor.", "sqrt(a/2)*sqrt(2)",
      "The radicand ${a}$ is twice a square. Write ${a}=2c^2$ with $c>0$; then $\\sqrt{{{a}}}=c\\sqrt2$.",
      lambda a: f"{isqrt(a//2)}*sqrt(2)", [2*a*a for a in range(2,14)])
    r("simplifying-radicals/kp2", "Simplify $3\\sqrt{{{a}}}$.", "(3*sqrt(a/5))*sqrt(5)",
      "Factor ${a}=5c^2$ with $c>0$. Extract $c$ and multiply by the outside coefficient: $3c\\sqrt5$.",
      lambda a: f"{3*isqrt(a//5)}*sqrt(5)", [5*a*a for a in range(2,14)])
    r("simplifying-radicals/kp3", "Use prime factorization to simplify $\\sqrt{{{a}}}$.", "sqrt(a/6)*sqrt(6)",
      "Factor ${a}=6c^2$. Pair the prime factors in $c^2$; one copy from each pair exits. The unpaired factors $2$ and $3$ remain as $c\\sqrt6$.",
      lambda a: f"{isqrt(a//6)}*sqrt(6)", [6*a*a for a in range(4,16)])
    r("simplifying-radicals-variables/kp1", "Simplify $\\sqrt{{{a}^2x^4}}$ for $x\\ge0$.", "a*x**2",
      "The positive coefficient ${a}$ exits the radical and the four factors of $x$ pair into $x^2$.",
      lambda a: f"{a}*x^2")
    r("simplifying-radicals-variables/kp2", "Simplify $\\sqrt{{{a}^2x^5}}$ for $x\\ge0$.", "a*x**2*sqrt(x)",
      "Write the radicand as $({a}x^2)^2x$. Extract the nonnegative square factor, leaving ${a}x^2\\sqrt x$.",
      lambda a: f"{a}*x^2*sqrt(x)")
    r("simplifying-radicals-variables/kp3", "Simplify $\\sqrt{{{a}x^6y^4}}$ for nonnegative $x,y$.", "sqrt(a)*x**3*y**2",
      "The coefficient ${a}$ is a square. The even powers contribute $x^3$ and $y^2$, so the result is $\\sqrt{{{a}}}x^3y^2$.",
      lambda a: f"{isqrt(a)}*x^3*y^2", [a*a for a in range(2,14)])


def operations(r):
    r("adding-subtracting-radicals/kp1", "Combine ${a}\\sqrt7+4\\sqrt7$.", "(a+4)*sqrt(7)",
      "Both terms have radical $\\sqrt7$. Add their coefficients to get $({a}+4)\\sqrt7$.", lambda a: f"{a+4}*sqrt(7)")
    r("adding-subtracting-radicals/kp2", "Simplify $\\sqrt{{{a}}}+\\sqrt{18}$ and combine.", "(sqrt(a/2)+3)*sqrt(2)",
      "Write ${a}=2c^2$ and $18=2\\cdot3^2$. The terms become $c\\sqrt2+3\\sqrt2=(c+3)\\sqrt2$.",
      lambda a: f"{isqrt(a//2)+3}*sqrt(2)", [2*a*a for a in range(2,14)])
    r("adding-subtracting-radicals/kp3", "Combine ${a}\\sqrt3+7\\sqrt3-2\\sqrt3$.", "(a+5)*sqrt(3)",
      "All three radicals match. Their signed coefficients total ${a}+7-2={a}+5$, giving $({a}+5)\\sqrt3$.", lambda a: f"{a+5}*sqrt(3)")
    r("radical-operations/kp1", "Simplify $\\sqrt5\\sqrt{{{a}}}$.", "sqrt(5*a)",
      "Both radicands are positive. Combine them under one root, $\\sqrt{{5\\cdot{a}}}$; since ${a}=5c^2$, this is $5c$.",
      lambda a: str(5*isqrt(a//5)), [5*a*a for a in range(2,14)])
    r("radical-operations/kp2", "Expand $\\sqrt2({a}+\\sqrt8)$.", "a*sqrt(2)+4",
      "Distribute: ${a}\\sqrt2+\\sqrt{16}={a}\\sqrt2+4$.", lambda a: f"{a}*sqrt(2)+4")
    r("radical-operations/kp3", "Multiply $({a}+\\sqrt3)({a}-\\sqrt3)$.", "a**2-3",
      "The cross terms cancel. Difference of squares leaves ${a}^2-(\\sqrt3)^2={a}^2-3$.", lambda a: str(a*a-3))
    r("dividing-radicals/kp1", "Simplify $\\sqrt{{{a}}}/\\sqrt3$.", "sqrt(a/3)",
      "The denominator is positive, so divide inside: $\\sqrt{{{a}/3}}$. Since ${a}=3c^2$, its principal root is $c$.",
      lambda a: str(isqrt(a//3)), [3*a*a for a in range(2,14)])
    r("dividing-radicals/kp2", "Simplify $\\sqrt{{{a}/50}}$.", "sqrt(a/2)/5",
      "Write ${a}=2c^2$ and $50=2\\cdot5^2$. Cancel the common factor two inside the fraction, giving $\\sqrt{{c^2/25}}=c/5$.",
      lambda a: str(Fraction(isqrt(a//2),5)), [2*a*a for a in range(2,14)])
    r("dividing-radicals/kp3", "Simplify ${a}\\sqrt{45}/(3\\sqrt5)$.", "a",
      "Extract $\\sqrt{45}=3\\sqrt5$. The quotient is ${a}(3\\sqrt5)/(3\\sqrt5)={a}$ because $3\\sqrt5\\ne0$.", lambda a: str(a))
    r("rationalizing-denominators/kp1", "Rationalize ${a}/\\sqrt7$.", "a*sqrt(7)/7",
      "Multiply numerator and denominator by $\\sqrt7$: ${a}\\sqrt7/(\\sqrt7)^2={a}\\sqrt7/7$.", lambda a: f"{Fraction(a,7)}*sqrt(7)")
    r("rationalizing-denominators/kp2", "Simplify and rationalize ${a}/\\sqrt{12}$.", "a*sqrt(3)/6",
      "First $\\sqrt{12}=2\\sqrt3$. Multiply by $\\sqrt3/\\sqrt3$ to obtain ${a}\\sqrt3/(2\\cdot3)={a}\\sqrt3/6$.", lambda a: f"{Fraction(a,6)}*sqrt(3)")
    r("rationalizing-denominators/kp3", "Rationalize ${a}/(4+\\sqrt3)$.", "a*(4-sqrt(3))/13",
      "Multiply by the conjugate $4-\\sqrt3$. The denominator is $16-3=13$, giving ${a}(4-\\sqrt3)/13$.", lambda a: f"{Fraction(4*a,13)}-{Fraction(a,13)}*sqrt(3)")


def advanced(r):
    r("radical-exponent-conversion/kp1", "Write the {a}th root of $x$ as a power of $x$, for $x>0$.", "x**(1/a)",
      "An index of {a} means a reciprocal exponent: raising $x^{{1/{a}}}$ to power {a} gives $x$.", lambda a: f"x^(1/{a})")
    r("radical-exponent-conversion/kp2", "Write $\\sqrt[3]{{x^{a}}}$ as a power of $x$, for $x>0$.", "x**(a/3)",
      "The inner power is {a} and the root index is three. Multiply the exponents to obtain $x^{{{a}/3}}$.", lambda a: f"x^({Fraction(a,3)})")
    r("radical-exponent-conversion/kp3", "Evaluate the unit fractional power ${a}^{{1/2}}$.", "sqrt(a)",
      "The denominator two specifies a square root. Find positive $c$ with $c^2={a}$; then ${a}^{{1/2}}=c$.",
      lambda a: str(isqrt(a)), [a*a for a in range(2,14)])
    r("rational-exponents/kp1", "Evaluate ${a}^{{3/2}}$ by taking the root first.", "sqrt(a)**3",
      "Find positive $c$ with $c^2={a}$. The half-power is $c$ and the numerator three cubes it, giving $c\\cdot c\\cdot c$.",
      lambda a: str(isqrt(a)**3), [a*a for a in range(2,14)])
    r("rational-exponents/kp2", "Evaluate ${a}^{{-3/2}}$ exactly.", "1/sqrt(a)**3",
      "Let $c>0$ satisfy $c^2={a}$. First cube its square root to obtain $c^3$; the negative exponent takes the reciprocal $1/c^3$.",
      lambda a: str(Fraction(1,isqrt(a)**3)), [a*a for a in range(2,14)])
    r("rational-exponents/kp3", "Simplify $x^{{{a}/4}}x^{{1/2}}$ for $x>0$.", "x**((a+2)/4)",
      "Use common denominator four: ${a}/4+1/2=({a}+2)/4$. This is the exponent of the product.", lambda a: f"x^({Fraction(a+2,4)})")
