"""Exact numeric root recipes with bounded, independently evaluated domains."""
from fractions import Fraction
from math import isqrt


def recipes(r):
    notation(r)
    roots(r)
    estimates(r)


def notation(r):
    decimals = [f"{a/10:.1f}" for a in range(11, 23)]
    r("scientific-notation-conversion/kp1", "Convert $({a})\\times10^4$ to an ordinary number.", "a*10000",
      "Multiplication by $10^4$ moves the decimal four places right: $({a})\\times10000$.",
      lambda a: str(Fraction(a)*10000), decimals)
    r("scientific-notation-conversion/kp2", "Write $({a})\\times10^{{-4}}$ as an ordinary number.", "a/10000",
      "The negative exponent divides by $10000$: $({a})/10000$. The decimal moves four places left.",
      lambda a: str(Fraction(a,)*Fraction(1,10000)), decimals)
    r("scientific-notation-conversion/kp3", "Which value is larger: $({a})\\times10^6$ or $9\\times10^5$? Give its value.", "a*1000000",
      "Rewrite the second value as $0.9\\times10^6$. Since $({a})>0.9$, the first value is larger.",
      lambda a: str(Fraction(a)*1000000), decimals)
    r("scientific-notation/kp1", "Compute $[({a})\\times10^2](2\\times10^3)$.", "a*200000",
      "Multiply coefficients to obtain $2({a})$, and add exponents $2+3=5$. Thus the value is $2({a})\\times10^5$; the coefficient is between $2.2$ and $4.4$.",
      lambda a: str(Fraction(a)*200000), decimals)
    r("scientific-notation/kp2", "Compute $[({a})\\times10^5]/(2\\times10^2)$.", "a*500",
      "Divide coefficients and subtract exponents: $(({a})/2)\\times10^3$. If that coefficient is below one, multiply it by ten and lower the exponent to two.",
      lambda a: str(Fraction(a)*500), decimals)
    r("scientific-notation/kp3", "Compute $[({a})\\times10^3](8\\times10^4)$, showing renormalization.", "a*80000000",
      "The product is $8({a})\\times10^7$. Since $8({a})$ lies between $16$ and $24.8$, rewrite it as $0.8({a})\\times10^8$.",
      lambda a: str(Fraction(a)*80000000), [f"{a/10:.1f}" for a in range(20,32)])
    r("scientific-notation-addition-subtraction/kp1", "Compute $[({a})\\times10^4]+(2\\times10^4)$.", "(a+2)*10000",
      "The powers already match. Add only the coefficients: $(({a})+2)\\times10^4$.",
      lambda a: str((Fraction(a)+2)*10000), decimals)
    r("scientific-notation-addition-subtraction/kp2", "Compute $[({a})\\times10^5]-(3\\times10^4)$.", "(a-3/10)*100000",
      "Match exponents: $3\\times10^4=0.3\\times10^5$. Subtract coefficients to obtain $(({a})-0.3)\\times10^5$.",
      lambda a: str((Fraction(a)-Fraction(3,10))*100000), [f"{a/10:.1f}" for a in range(20,32)])
    r("scientific-notation-addition-subtraction/kp3", "Compute $[({a})\\times10^4]+(8\\times10^4)$ and renormalize.", "(a+8)*10000",
      "Add coefficients: $(({a})+8)\\times10^4$. The sum lies between ten and twenty, so the normalized form is $((({a})+8)/10)\\times10^5$.",
      lambda a: str((Fraction(a)+8)*10000), [f"{a/10:.1f}" for a in range(21,33)])


def roots(r):
    squares = [a*a for a in range(1,13)]
    r("perfect-square-roots/kp1", "A square mosaic has {a} tiles on each side. How many tiles does it contain?", "a**2",
      "There are {a} rows with {a} tiles each, so the total is ${a}\\cdot{a}={a}^2$.",
      lambda a: str(a*a), list(range(1,13)))
    r("perfect-square-roots/kp2", "A nonnegative length $s$ has $s^2={a}$. Find $s$.", "sqrt(a)",
      "The length is nonnegative, so take the principal root $s=\\sqrt{{{a}}}$. Squaring this value recovers the given square ${a}$.",
      lambda a: str(isqrt(a)), squares)
    r("perfect-square-roots/kp3", "A square sheet has area ${a}$ square units. Find its positive side length.", "sqrt(a)",
      "Area is side times side: $s^2={a}$. Hence $s=\\sqrt{{{a}}}>0$; the negative algebraic root cannot be a length.",
      lambda a: str(isqrt(a)), [a*a for a in range(9,21)])
    r("square-roots/kp1", "Evaluate $3+\\sqrt{{{a}}}$.", "3+sqrt(a)",
      "Find the nonnegative factor whose square is ${a}$, then add three: $3+\\sqrt{{{a}}}$.",
      lambda a: str(3+isqrt(a)), squares)
    r("square-roots/kp2", "Find the positive $s$ satisfying $s^2={a}/169$.", "sqrt(a)/13",
      "The denominator has root $13$ because $13^2=169$. The fraction therefore has positive root $\\sqrt{{{a}}}/13$.",
      lambda a: str(Fraction(isqrt(a),13)), squares)
    r("square-roots/kp3", "Multiply $\\sqrt{{{a}}}$ by $\\sqrt9$.", "3*sqrt(a)",
      "The second root is three. The product rule gives $\\sqrt{{9\\cdot{a}}}=3\\sqrt{{{a}}}$.",
      lambda a: str(3*isqrt(a)), squares)
    # The ten positive cubes up to 1000 expose a real domain-cardinality blocker.
    r("cube-roots/kp1", "Evaluate the real cube root $\\sqrt[3]{{{a}}}$.", "a**(1/3)",
      "Find the real factor $c$ whose threefold product is ${a}$. Its cube equals the radicand, so $c=\\sqrt[3]{{{a}}}$.",
      lambda a: str(next(n for n in range(-10,11) if n**3 == a)),
      [n**3 for n in range(1,11)])
    r("cube-roots/kp2", "Find the real solution of $t^3=-{a}$.", "-(a**(1/3))",
      "First take the positive cube root of ${a}$. Negating it makes the product of three equal factors negative: $t=-\\sqrt[3]{{{a}}}$.",
      lambda a: str(-next(n for n in range(1,20) if n**3 == a)), [a**3 for a in range(1,13)])
    r("cube-roots/kp3", "Find the principal fourth root of ${a}$.", "sqrt(sqrt(a))",
      "The principal fourth root is nonnegative. If $c^2=\\sqrt{{{a}}}$, then $c^4={a}$, so take the square root twice.",
      lambda a: str(next(n for n in range(1,20) if n**4 == a)), [a**4 for a in range(1,13)])


def estimates(r):
    numbers = [n*n+1 for n in range(2,14)]
    r("estimating-square-roots/kp1", "Locate $\\sqrt{{{a}}}$ between consecutive integers; give the lower endpoint.",
      "sqrt(a-1)", "Here ${a}-1$ is a square, say $m^2$. Since $m\\ge2$, $m^2<{a}<(m+1)^2$. Taking positive roots gives lower endpoint $m=\\sqrt{{{a}-1}}$.",
      lambda a: str(isqrt(a)), numbers)
    r("estimating-square-roots/kp2", "Round the positive square root of ${a}$ to the nearest integer.",
      "sqrt(a-1)", "Here $m=\\sqrt{{{a}-1}}\\ge2$. The squared midpoint is $m^2+m+1/4>{a}=m^2+1$, so the root lies below $m+1/2$ and rounds to $m$.",
      lambda a: str(isqrt(a)+(4*a>(2*isqrt(a)+1)**2)), numbers)
    r("estimating-square-roots/kp3", "Which is larger, $\\sqrt{{{a}}}$ or $15$? Give the larger value.", "15",
      "Both are nonnegative, so compare squares: ${a}<225=15^2$. Therefore $15$ is larger.",
      lambda a: "15", numbers)
