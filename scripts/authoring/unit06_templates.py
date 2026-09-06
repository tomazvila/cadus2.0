"""Write real local-import template candidates for every unit06 objective.

Run the paired Rust verifier before treating a candidate as pending evidence.
Rejected candidates remain explicit schema blockers; this program never imports
or approves content and does not change the course audit.
"""
import json
from pathlib import Path

import unit06_template_exponents as exponents
import unit06_template_radicals as radicals
import unit06_template_roots as roots
from unit06_template_derivations import enrich

OUT = Path("docs/content-foundations/unit06-correction")
CANDIDATES = Path("crates/worker/tests/fixtures/unit06-template-candidates.json")
HINTS = {
    "exponent-product-rule": "For equal bases, multiplication combines their exponent counts.",
    "exponent-quotient-rule": "Cancel matching factors in numerator and denominator; the base must be nonzero.",
    "exponent-product-quotient-rules": "Collect exponent contributions for each base separately, subtracting those in the denominator.",
    "power-of-a-power-rule": "A power outside a power repeats the inner group; count all copies of the base.",
    "power-rule-exponents": "Apply the outside power to every factor, including the coefficient.",
    "zero-exponent-rule": "Relate the zero exponent to a quotient of equal powers with a nonzero base.",
    "negative-zero-exponents": "A negative exponent moves its base across the fraction bar; it does not change the sign of the base.",
    "simplifying-negative-exponent-expressions": "Apply outside powers first, then combine exponents and move negative powers across the fraction bar.",
    "scientific-notation-conversion": "The sign of the power of ten determines which way to move the decimal point.",
    "scientific-notation": "Operate on the coefficients separately from the powers of ten, then check coefficient normalization.",
    "scientific-notation-addition-subtraction": "Express both terms using the same power of ten before adding or subtracting their coefficients.",
    "perfect-square-roots": "A square is an equal-factor product; a principal square root selects its nonnegative factor.",
    "square-roots": "Evaluate the principal roots first, then perform the operation surrounding them.",
    "estimating-square-roots": "Bracket the radicand by consecutive squares; for rounding, compare with the square of the midpoint.",
    "cube-roots": "Verify a proposed root by raising it to the root index; odd roots preserve the sign.",
    "simplifying-radicals": "Factor the radicand into its largest perfect-square factor and the remaining squarefree part.",
    "simplifying-radicals-variables": "Pair repeated variable factors under the root and use the stated sign assumptions when extracting them.",
    "adding-subtracting-radicals": "Simplify the radicals, then combine coefficients only for terms with the same radical part.",
    "radical-operations": "Distribute each factor, combine products of roots, and look for cancelling conjugate terms.",
    "dividing-radicals": "Separate the coefficient quotient from the radical quotient and simplify common nonzero factors.",
    "rationalizing-denominators": "Choose a radical or conjugate multiplier that makes the denominator rational, and multiply both numerator and denominator.",
    "radical-exponent-conversion": "The denominator of a fractional exponent is the root index and its numerator is the power.",
    "rational-exponents": "Take the indicated root before the integer power; a negative exponent takes a reciprocal.",
    "pythagorean-theorem": "Identify the hypotenuse before deciding whether to add or subtract the known squared lengths.",
    "pythagorean-converse": "Compare the square of the longest side with the sum of the other squared lengths.",
    "radical-equations-basic": "Isolate the radical, check the sign of the other side, then square and substitute back.",
}


def escaped(text):
    """Escape LaTeX braces once, preserving the single parameter placeholder."""
    text = text.replace("{{", "{").replace("}}", "}")
    # Protect nested parameter braces before doubling ordinary LaTeX groups.
    return text.replace("{a}", "\x01").replace("{", "{{").replace("}", "}}").replace("\x01", "{a}")


def geometry(r):
    r("pythagorean-theorem/kp1", "A right triangle has legs $3\\cdot{a}$ and $4\\cdot{a}$ cm. Find its hypotenuse in cm.", "5*a",
      "The sum of the leg squares is $(9+16){a}^2=25{a}^2$. The positive square root is $5\\cdot{a}$ cm.", lambda a: str(5*a))
    r("pythagorean-theorem/kp2", "A right triangle has hypotenuse $13\\cdot{a}$ and one leg $5\\cdot{a}$ cm. Find the other leg in cm.", "12*a",
      "Subtract leg square from hypotenuse square: $(169-25){a}^2=144{a}^2$. The positive length is $12\\cdot{a}$ cm.", lambda a: str(12*a))
    r("pythagorean-theorem/kp3", "An isosceles right triangle has two legs of length {a}. Find its exact hypotenuse.", "a*sqrt(2)",
      "The two leg squares total $2{a}^2$. Taking the positive root gives ${a}\\sqrt2$.", lambda a: f"{a}*sqrt(2)")
    r("pythagorean-converse/kp1", "A triangle has sides $5\\cdot{a}$, $12\\cdot{a}$, and $13\\cdot{a}$. Is it right?", "equalitylabel((5*a)**2+(12*a)**2,(13*a)**2)",
      "The largest side has square $169{a}^2$; the other squares sum to $(25+144){a}^2=169{a}^2$, so yes.", lambda a: "yes")
    r("pythagorean-converse/kp2", "A triangle has sides $3\\cdot{a}$, $4\\cdot{a}$, and $6\\cdot{a}$. Is it right?", "equalitylabel((3*a)**2+(4*a)**2,(6*a)**2)",
      "The longest side has square $36{a}^2$ but the other squares total $25{a}^2$. Their inequality proves the triangle is not right.", lambda a: "no")
    r("pythagorean-converse/kp3", "Classify a triangle with sides $4\\cdot{a}$, $5\\cdot{a}$, and $6\\cdot{a}$. Enter $1$ for acute, $2$ for right, or $3$ for obtuse.", "acute",
      "The sum of the smaller squares is $41{a}^2$, greater than the largest square $36{a}^2$, so all its angles are acute.", lambda a: "acute")
    r("radical-equations-basic/kp1", "Solve $\\sqrt x={a}$ and verify the solution.", "a**2",
      "Square both sides to obtain $x={a}^2$. Substituting gives $\\sqrt{{{a}^2}}={a}$ since {a} is positive.", lambda a: str(a*a))
    r("radical-equations-basic/kp2", "Solve $2\\sqrt x+3=2\\cdot{a}+3$.", "a**2",
      "Subtract three and divide by two: $\\sqrt x={a}$. Square to get $x={a}^2$; substitution recovers $2\\cdot{a}+3$.", lambda a: str(a*a))
    r("radical-equations-basic/kp3", "Solve $\\sqrt x=-{a}$ over the reals and check the sign condition. Enter $0$ for no solution or $1$ for one solution.", "no solution",
      "A principal square root is nonnegative while $-{a}<0$. Squaring would give ${a}^2$, whose root is positive {a}, so that candidate is extraneous.", lambda a: "no solution")


def generate():
    rows = []

    def add(key, statement, expression, sketch, oracle, values=None):
        values = list(range(2,14)) if values is None else values
        samples = [{"params": {"a": a}, "expected": oracle(a)} for a in values]
        arguments = dict(statement=escaped(statement), answer_expr=expression,
                         solution_sketch=escaped(sketch), params={"a": {"kind": "choice", "values": values}},
                         constraints=[], hints=[HINTS[key.split("/")[0]]],
                         distractors=[], samples=samples)
        encoded_labels = {
            "pythagorean-converse/kp3": "1",
            "radical-equations-basic/kp3": "0",
        }
        if key in encoded_labels:
            label = encoded_labels[key]
            arguments["answer_expr"] = label
            for sample in arguments["samples"]:
                sample["expected"] = label
            arguments["answer_contract"] = {"kind": "exact"}
        if key in {"radical-equations-basic/kp1", "radical-equations-basic/kp2"}:
            arguments["answer_contract"] = {"kind": "exact"}
        rows.append(dict(kp_id=key, kind="template", arguments=arguments))

    for module in (exponents, roots, radicals):
        module.recipes(add)
    geometry(add)
    keys = json.loads(Path("crates/core/tests/fixtures/exponents_radicals_unit_kps.json").read_text())
    assert sorted(row["kp_id"] for row in rows) == sorted(keys)
    return enrich(rows)


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    CANDIDATES.write_text(json.dumps(generate(), indent=2) + "\n")
