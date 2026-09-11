"""Production-bound inequality templates with exhaustive, independently solved samples."""
import itertools
import json
from pathlib import Path

from .inequalities_answers import reconstruct

ROOT = Path(__file__).resolve().parents[3]
UNION = {"kind": "inequality_union"}
EXACT = {"kind": "exact"}
YES_NO = {"kind": "label", "options": [["yes"], ["no"]]}
OPS = ["<", "<=", ">", ">="]


def recipe(key, statement, expr, axes, contract, hints, sketch):
    samples = []
    for values in itertools.product(*axes.values()):
        params = dict(zip(axes, values))
        text = statement.format(**params)
        samples.append({"params": params, "expected": reconstruct(key, text)})
    arguments = dict(statement=statement, answer_expr=expr, answer_contract=contract,
                     params={name: {"kind": "choice", "values": values}
                             for name, values in axes.items()}, constraints=[],
                     hints=hints, solution_sketch=sketch, distractors=[], samples=samples)
    return {"kp_id": key, "kind": "template", "arguments": arguments}


def classifications():
    return [recipe(
        "graphing-inequalities-number-line/kp1",
        "For $x {o} {b}$, is its number-line endpoint circle open or closed?",
        "boundarycircle(o)", {"o": OPS, "b": [-19, -13, 17]},
        {"kind": "label", "options": [["closed"], ["open"]]},
        ["Check whether the comparison permits equality at the endpoint.",
         "A filled circle records membership of the endpoint itself."],
        "For $x {o} {b}$, test $x={b}$. An inclusive sign admits equality and requires a closed circle; a strict sign excludes equality and requires an open circle."),
        recipe(
        "graphing-linear-inequalities/kp1",
        "For $y {o} {a}x+7$, is the boundary line solid or dashed?",
        "boundarystyle(o)", {"o": OPS, "a": [-5, 4, 6]},
        {"kind": "label", "options": [["dashed"], ["solid"]]},
        ["The boundary is obtained by replacing the comparison with equality.",
         "Decide whether points on that equality line belong to the solution region."],
        "The boundary is $y={a}x+7$. Equality is allowed by an inclusive comparison, giving a solid line; a strict comparison excludes the line and uses a dashed line."),
        recipe(
        "solutions-of-inequalities/kp1",
        "Is the integer comparison ${a} < {b}$ true or false? Answer {j} or {l}.",
        "signcase(a-b,[j,l,l])",
        {"a": [-18, -12, -6], "b": [-15, -12, -9, -3],
         "j": ["true"], "l": ["false"]},
        {"kind": "label", "options": [["true"], ["false"]]},
        ["Locate the two integers on a number line.",
         "The strict less-than sign requires the left integer to lie farther left."],
        "Compare ${a}$ and ${b}$ in numerical order. If the left integer is smaller the statement is true; equality or a greater left integer makes a strict comparison false."),
        recipe(
        "graphing-linear-inequalities/kp2",
        "Does the point $({p},{q})$ belong to the shaded region $y <= 3x-11$? Answer {l} or {j}.",
        "signcase(3*p-11-q,[j,l,l])",
        {"p": [-3, 2, 5], "q": [-20, -5, 4, 10],
         "j": ["no"], "l": ["yes"]}, YES_NO,
        ["Substitute the point's first coordinate for x and its second for y.",
         "Compare the point's height with the boundary height at that x-coordinate."],
        "The boundary height at $x={p}$ is $3({p})-11$. Compare ${q}$ with that height; a point below or on the boundary belongs to this inclusive lower half-plane."),
        recipe(
        "checking-a-solution/kp1",
        "Is $x={c}$ a solution of $7x-11={d}$? Answer yes or no.",
        "equalitylabel(7*c-11,d)", {"c": [-6, -3, 2], "d": [-53, -32, 3, 24]},
        YES_NO, ["Replace every occurrence of x by the proposed value.",
                 "Evaluate the multiplication before subtracting the constant, then compare both sides."],
        "Substitution gives $7({c})-11$ on the left and ${d}$ on the right. The candidate is a solution exactly when these two evaluated integers are equal.")]


def rays_and_context():
    return [recipe(
        "graphing-inequalities-number-line/kp3",
        "A number line has a closed circle at ${b}$ and a ray extending left. Write its inequality in {s}.",
        "upperbound(s,b)",
        {"b": [-23, -21, -19, -17, -15, -13, 11, 13, 15, 17, 19, 21],
         "s": ["x"]}, UNION,
        ["Identify whether the ray represents values smaller or larger than its endpoint.",
         "Use the endpoint's circle to decide whether equality is allowed."],
        "The leftward ray represents values below ${b}$. The closed circle includes ${b}$, so combine the less-than comparison with equality."),
        recipe(
        "writing-inequalities-from-statements/kp1",
        'Translate the phrase "${s}$ is at least ${b}$" into an inequality.',
        "lowerbound(s,b)",
        {"b": [13, 14, 16, 17, 19, 21, 23, 24, 26, 27, 29, 31], "s": ["x"]},
        UNION, ["Identify which side of the stated threshold the phrase permits.",
                "Decide whether reaching the threshold exactly satisfies the phrase."],
        '"At least ${b}$" permits ${b}$ and every greater value. Use a greater-than comparison with equality.'),
        recipe(
        "writing-inequalities-from-statements/kp3",
        "A training course requires $n >= {b}$ attendance hours. What is the fewest attendance hours that qualifies? Give the answer in h.",
        "b", {"b": [13, 14, 16, 17, 19, 21, 23, 24, 26, 27, 29, 31]},
        {"kind": "unit", "quantity": "time", "unit": "h"},
        ["Identify the boundary of the acceptable attendance values.",
         "Check whether the boundary itself is allowed, then state the minimum with its unit."],
        "The inclusive lower bound admits the boundary itself. Every smaller attendance value fails the requirement; the minimum is the boundary value, measured in hours.")]


def intervals_and_systems():
    return [recipe(
        "compound-inequalities/kp1",
        "Solve the three-part inequality ${a} <= 3x+5 < {b}$. Give interval notation.",
        "[(a-5)/3,(b-5)/3)", {"a": [-19, -13, -7], "b": [20, 26, 32, 38]},
        EXACT, ["Undo the added constant across all three parts together.",
                "Divide all three parts by the positive coefficient, preserving both comparisons."],
        "Subtract 5 from all three parts, giving ${a}-5 <= 3x < {b}-5$. Divide every part by positive 3. The left endpoint stays included and the right endpoint stays excluded."),
        interval_recipe("kp2", [
            "x < -23", "x <= -19", "x > -17", "x >= -13", "x < 11", "x >= 17",
            "(-∞, -29)", "(-∞, -27]", "(-21, ∞)",
            "[13, ∞)", "(-∞, 19]", "[23, ∞)"]),
        interval_recipe("kp3", [
            "x < -19 or x > 13", "x <= -17 or x > 23", "x < -11 or x >= 29",
            "x <= -23 or x >= 31", "x < -29 or x > 19", "x <= -31 or x >= 17",
            "(-∞, -13) ∪ (11, ∞)",
            "(-∞, -15] ∪ (21, ∞)",
            "(-∞, -21) ∪ [27, ∞)",
            "(-∞, -25] ∪ [33, ∞)",
            "(-∞, -27) ∪ (35, ∞)",
            "(-∞, -33] ∪ [37, ∞)"]),
        recipe(
        "substitution-with-isolated-variable/kp2",
        "Solve $y=5x+{a}$ and $y=2x+{b}$ by setting the expressions for y equal. Give $(x,y)$.",
        "((b-a)/3,5*(b-a)/3+a)", {"a": [-15, -9, -3], "b": [18, 24, 30, 36]},
        {"kind": "coordinates", "arity": 2},
        ["Both right-hand sides equal y, so equate them to eliminate y.",
         "Collect x-terms on one side and constants on the other; then substitute into either original expression."],
        "Equate $5x+({a})=2x+{b}$, giving $3x={b}-({a})$. Divide by 3 and substitute the resulting x into either equation. Verify that both original expressions give the same y.")]


def interval_recipe(kp, sources):
    return recipe(
        f"interval-notation/{kp}",
        "Convert ${s}$ to the other notation: give interval notation for an inequality input, or inequalities in x for an interval input. Preserve any ∪ as or.",
        "convertnotation(s)", {"s": sources}, UNION,
        ["Identify the finite endpoints and the direction of each ray.",
         "A finite endpoint is included exactly when its comparison allows equality or its bracket is square. Infinity always takes a round bracket."],
        "Read the endpoints in ${s}$. A left ray extends toward negative ∞ and a right ray toward positive ∞. Transfer each finite endpoint's inclusion unchanged between its comparison sign and its bracket. Join separate rays with ∪ or the word or.")


def build():
    return classifications() + rays_and_context() + intervals_and_systems()


if __name__ == "__main__":
    target = ROOT / "docs/content-foundations/template36/inequalities.json"
    target.write_text(json.dumps(build(), indent=2) + "\n")
