"""Production-importable bounded expression/formula recipes; pending only."""
import itertools
import json
import re
from pathlib import Path

from expressions_oracle import reconstruct

ROOT = Path(__file__).resolve().parents[3]
EXACT = {"kind": "exact"}


def recipe(key, statement, answer, axes, sketch, hints, contract=EXACT):
    if key.startswith(("literal-equations/", "rearranging-formulas/")):
        statement, answer, axes, sketch = symbolic_operands(statement, answer, axes, sketch)
    samples = []
    for values in itertools.product(*axes.values()):
        params = dict(zip(axes, values))
        problem = statement.format(**params)
        samples.append({"params": params, "expected": reconstruct(key, problem)})
    return {"kp_id": key, "kind": "template", "arguments": {
        "statement": statement, "answer_expr": answer,
        "answer_contract": contract,
        "params": {name: {"kind": "choice", "values": list(values)}
                   for name, values in axes.items()},
        "constraints": [], "samples": samples, "solution_sketch": sketch,
        "hints": hints, "distractors": [],
    }}


def symbolic_operands(statement, answer, axes, sketch):
    """Declare visible letters explicitly for the multi-step production contract."""
    names = {"y": ("j", "z"), "b": ("k", "u"), "d": ("l", "v"), "e": ("m", "w")}
    axes = dict(axes)
    for original, (parameter, letter) in names.items():
        if not re.search(rf"\b{original}\b", answer):
            continue
        axes[parameter] = [letter]
        answer = re.sub(rf"\b{original}\b", f"symbol({parameter})", answer)
        for field in ("statement", "sketch"):
            value = statement if field == "statement" else sketch
            value = re.sub(r"\$([^$]+)\$", lambda match: "$" + re.sub(
                rf"\b{original}\b", "{" + parameter + "}", match[1]) + "$", value)
            if field == "statement":
                statement = value
            else:
                sketch = value
    return statement, answer, axes, sketch


def literal():
    return [
        recipe("literal-equations/kp1",
               "Solve $y={a}b*x+{c}d-e$ for $x$, assuming $b$ is nonzero. "
               "Keep the other letters symbolic and give the expression equal to $x$.",
               "(y-c*d+e)/(a*b)", {"a": [13, 14, 15, 16], "c": [2, 3, 4]},
               "Add $e$ and subtract ${c}d$ on both sides: $y+e-{c}d={a}b*x$. "
               "Divide by the nonzero coefficient ${a}b$ to isolate $x$.",
               ["Move every term without the target variable to the other side.",
                "Divide the resulting side by the entire coefficient of the target."]),
        recipe("literal-equations/kp2",
               "Solve ${a}b*x+{c}d*x=y$ for $x$, assuming ${a}b+{c}d$ is nonzero. "
               "Keep the other letters symbolic and give the expression equal to $x$.",
               "y/(a*b+c*d)", {"a": [17, 19, 23, 29], "c": [3, 5, 7]},
               "The common factor $x$ gives $({a}b+{c}d)x=y$. "
               "Divide by the stated nonzero sum of coefficients.",
               ["Both terms contain the target variable; factor it out.",
                "Treat the full coefficient sum as one divisor."]),
        recipe("literal-equations/kp3",
               "Solve $y=b*(x+{a}d)/{c}$ for $x$, assuming $b$ is nonzero. "
               "Keep the other letters symbolic and give the expression equal to $x$.",
               "c*y/b-a*d", {"a": [11, 13, 17, 19], "c": [5, 7, 9]},
               "Clear the fraction: ${c}y=b(x+{a}d)$. Divide by $b$ to obtain "
               "$({c}y)/b=x+{a}d$, then subtract the term ${a}d$.",
               ["First undo the division outside the parentheses.",
                "After removing the outer factor, isolate the target inside the sum."]),
    ]


def formulas():
    return [
        recipe("rearranging-formulas/kp1",
               "Solve $y=x+{a}b$ for $x$. Keep $y$ and $b$ symbolic; "
               "give the expression equal to $x$.",
               "y-a*b", {"a": range(21, 33)},
               "The added term is ${a}b$. Subtract that whole term from both sides "
               "to leave the target by itself.",
               ["Identify the term added to the target variable.",
                "Apply the inverse operation to both sides of the equation."]),
        recipe("rearranging-formulas/kp2",
               "Solve $y={a}x+{c}b$ for $x$. Keep $y$ and $b$ symbolic; "
               "give the expression equal to $x$.",
               "(y-c*b)/a", {"a": [17, 19, 23, 29], "c": [11, 13, 15]},
               "Subtract ${c}b$: $y-{c}b={a}x$. Then divide both sides by "
               "${a}$. Substitution of this expression restores the original formula.",
               ["Undo the addition before undoing multiplication.",
                "Divide the whole remaining expression by the target's coefficient."]),
    ]


def parts_and_substitution():
    contract = {"kind": "multipart", "parts": [
        {"name": name, "contract": EXACT}
        for name in ("terms", "coefficient", "constant")
    ]}
    return [
        recipe("parts-of-an-expression/kp1",
               "For $4x+({a})y+({b})$, report terms = the number of terms; "
               "coefficient = the coefficient of $y$; constant = the constant term.",
               "multipart(3,a,b)", {"a": [-9, -7, -4, 2], "b": [-8, -3, 6]},
               "The separate summands are $4x$, $({a})y$, and $({b})$. "
               "The numerical factor multiplying $y$ is ${a}$, and the summand "
               "with no letter is ${b}$. Keep each sign attached.",
               ["Count the separate summands, keeping a parenthesized sign with its term.",
                "Read the signed numerical factor of the requested variable, then the term with no variable."],
               contract),
        recipe("substituting-values/kp1",
               "Evaluate ${a}x+({b})$ at $x={v}$.",
               "a*v+b", {"a": [6, 7, 8], "b": [-8, -4], "v": [11, 13]},
               "Replace $x$ with ${v}$ to get ${a}({v})+({b})$. "
               "Multiply first, then add the signed constant.",
               ["Replace the variable with the given input in parentheses.",
                "Complete multiplication before applying the constant term."]),
    ]


def translations():
    return [
        recipe("translating-phrases-to-expressions/kp1",
               "Write an expression for the difference of $x$ and ${a}$, in that order.",
               "x-a", {"a": range(21, 33)},
               "The first named quantity is $x$ and the second is ${a}$. "
               "A difference subtracts the second quantity from the first.",
               ["Identify which quantity is named first.",
                "Use subtraction and preserve the stated order."]),
        recipe("translating-phrases-to-expressions/kp2",
               "Write an expression for the quotient of $x$ and ${a}$, in that order.",
               "x/a", {"a": range(21, 33)},
               "A quotient divides the first named quantity by the second. "
               "Here $x$ is the numerator and ${a}$ is the nonzero denominator.",
               ["A quotient represents division.",
                "Place the first quantity above the second quantity in a fraction."]),
        recipe("translating-phrases-to-expressions/kp3",
               "Write an expression for ${a}$ less than ${b}$ times the number $x$.",
               "b*x-a", {"a": [21, 23, 27, 29], "b": [6, 7, 8]},
               "First form the product ${b}x$. The phrase '${a}$ less than' "
               "subtracts ${a}$ from this product, so the product comes first.",
               ["Translate the multiplication phrase first.",
                "For 'less than', subtract the stated amount from that product."]),
        recipe("translating-sentences-to-equations/kp1",
               "Let $x$ be a number. ${a}$ times the number minus ${b}$ is ${c}$. "
               "Translate the sentence into an equation and find the number. Give its value.",
               "(c+b)/a", {"a": [3, 6], "b": [18, 24], "c": [120, 180, 240]},
               "The sentence becomes ${a}x-{b}={c}$. Add ${b}$ to both sides, "
               "then divide the total ${c}+{b}$ by ${a}$. Check by multiplying "
               "the result and subtracting the stated amount.",
               ["The word 'is' separates the two equal sides.",
                "Reverse subtraction, then reverse multiplication."]),
        recipe("writing-expressions-from-patterns/kp2",
               "A tool library charges a one-time joining fee of €{a} and €{b} "
               "for each rental. Write the total cost in euros after $n$ rentals.",
               "a+b*n", {"a": [61, 67, 73], "b": [11, 13, 17, 19]},
               "The fee €{a} is paid once. The rentals cost €{b} each, so "
               "$n$ rentals contribute ${b}n$. Add the fixed and changing parts; "
               "at zero rentals the expression must equal the joining fee.",
               ["Separate the one-time payment from the payment repeated for each rental.",
                "Multiply the per-rental rate by the number of rentals, then include the fixed payment."]),
    ]


def build():
    return literal() + formulas() + parts_and_substitution() + translations()


def main():
    rows = build()
    path = ROOT / "docs/content-foundations/template36/expressions.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(rows, ensure_ascii=False, indent=2) + "\n")
    print(f"{len(rows)} recipes; {sum(len(r['arguments']['samples']) for r in rows)} samples")


if __name__ == "__main__":
    main()
