"""Build Unit04-only pending recipes and premises; preserve unrelated YAML bytes."""
import itertools
import json
import re
from fractions import Fraction as Q
from pathlib import Path

from unit04_residual_data import AUTHORED, line, standard
from math import gcd

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / "docs/content-foundations/unit04-residual"

# Every axis changes a mathematical input and its correct output. The small
# Cartesian domains are exhaustive; no text-choice axis pads the domain floor.
RECIPES = [
    ("plotting-points/kp1", [1,2,3,4], [2,3,4], "(a+2,b+1)",
     "A marker starts at $(2,1)$, moves ${a}$ units right and ${b}$ units up. Give its final ordered pair.",
     "Add the horizontal displacement to $2$ and vertical displacement to $1$: the coordinates are $(2+{a},1+{b})$.",
     "Track horizontal and vertical displacements separately."),
    ("constant-of-proportionality/kp1", [7,11,13,17], [2,3,5], "a/b",
     "In a proportional relationship, input ${b}$ corresponds to output ${a}$. Find the constant multiplying input to produce output.",
     "The multiplier $k$ satisfies ${b}k={a}$, so divide the output ${a}$ by the input ${b}$.",
     "Determine how many output units correspond to one input unit."),
    ("slope-from-a-graph/kp1", [2,3,4], [5,7,11,13], "a/b",
     "A graph's marked slope triangle rises ${a}$ vertical units over a rightward run of ${b}$ horizontal units. Find the slope.",
     "The vertical change ${a}$ is positive and the horizontal change is ${b}$. Their quotient gives the rise per unit of run.",
     "Place vertical change above horizontal change."),
    ("slope/kp1", [-7,-5,-3,2], [4,8,16], "a/b",
     "A line's directed vertical change is ${a}$ units for a rightward horizontal change of ${b}$ units. Give the slope as a reduced fraction.",
     "Use the signed vertical change ${a}$ over the positive run ${b}$. Reduce the quotient without discarding its sign.",
     "A negative vertical change indicates a falling line."),
    ("reading-slope-intercept-equations/kp1", [-3,-1,2,4], [-5,0,6], "(a,b)",
     "Read the coefficient record $(m,b)$ for $y=({a})x+({b})$.",
     "The coefficient multiplying $x$ gives the slope ${a}$; setting $x=0$ leaves the intercept value ${b}$.",
     "Keep each coefficient's sign and report the slope first."),
    ("slope-intercept-form/kp1", [-3,-1,2,4], [-5,1,6], "(a,b-2*a)",
     "A line has slope ${a}$ and passes through $(2,{b})$. Write its slope-intercept equation.",
     "Use $y=mx+c$ with $m={a}$. The point gives ${b}=2({a})+c$, so the constant is ${b}-2({a})$.",
     "Insert the point into a line equation with the given slope to determine the constant."),
    ("slope-intercept-form/kp2", [-4,-1,3,7], [-3,2,6], "((b-a)/3,(2*a+b)/3)",
     "Determine the slope-intercept equation of the line through $(-1,{a})$ and $(2,{b})$.",
     "The run is $2-(-1)=3$ and rise is ${b}-({a})$. After finding their quotient $m$, use ${a}=-m+c$ to determine the intercept $c$.",
     "Find the slope from both points, then use either point to determine the intercept."),
    ("slope-intercept-form/kp3", [-6,-3,2,5], [-5,1,4], "(-b/a,b)",
     "A line's horizontal-axis crossing is $({a},0)$ and its vertical-axis crossing is $(0,{b})$. Write the slope-intercept equation.",
     "The vertical-axis crossing supplies the constant ${b}$. The slope from $(0,{b})$ to $({a},0)$ is $(0-({b}))/({a}-0)$.",
     "Use the vertical intercept as the constant and both intercepts to find the slope."),
    ("point-slope-form/kp3", [-3,-1,2,4], [-5,1,6], "(a,3*a+b)",
     "Convert $y-({b})=({a})(x+3)$ to slope-intercept form.",
     "Distribute ${a}$ over $x+3$, then add ${b}$ to both sides. The constant becomes $3({a})+({b})$.",
     "Distribute the multiplier before isolating the vertical coordinate."),
    ("parallel-perpendicular-lines/kp2", [-4,-2,2,4], [-5,1,6], "(-1/a,b+2/a)",
     "Find the slope-intercept equation perpendicular to $y=({a})x+3$ through $(2,{b})$.",
     "The perpendicular slope is $-1/({a})$. Substitute $(2,{b})$ to get ${b}=(-1/({a}))2+c$, then solve for $c$.",
     "The slopes of perpendicular nonvertical lines multiply to negative one."),
]

RECIPES.extend([
    ("proportional-relationships/kp1", [7,11,13,17], [2,3,5], "a*b",
     "The proportional equation is $y=({a})x$. Use the equation to find $y$ at $x={b}$.",
     "The coefficient ${a}$ multiplies the input ${b}$, so the output is the product ${a}\\cdot{b}$.",
     "Replace the input variable with the given value before multiplying."),
    ("graphing-proportional-relationships/kp2", [5,7,11,13], [2,3,4], "a/b",
     "A straight graph through the origin has a marked point whose projections onto the input and output axes read $x={b}$ and $y={a}$. Find $k$ in $y=kx$.",
     "The projections identify the point $({b},{a})$. A proportional graph satisfies ${a}=k({b})$, so divide the vertical coordinate by the horizontal coordinate.",
     "Read horizontal input first and vertical output second."),
    ("slope-from-a-graph/kp2", [2,3,4], [5,7,11,13], "-a/b",
     "A descending graph drops ${a}$ vertical units over a rightward run of ${b}$ units. Find its signed slope.",
     "Left-to-right descent makes the vertical change $-{a}$. The run stays positive, so divide $-{a}$ by ${b}$.",
     "First decide the sign of the vertical change as you move right."),
    ("point-slope-form/kp1", [-4,-1,3,6], [-5,2,7], "(-b,-2,-a)",
     "Use slope $-2$ and point $({a},{b})$ to complete $y+A=M(x+B)$. Enter $(A,M,B)$, including signs.",
     "Point-slope form subtracts the given coordinates: $y-({b})=-2(x-({a}))$. Thus the added offsets are the negatives of ${b}$ and ${a}$.",
     "Subtract each point coordinate from its corresponding variable."),
    ("point-slope-standard-form/kp1", [1,2,3,4], [-5,1,6],
     "(a/gcd(a,2),-2/gcd(a,2),(-3*a-2*b)/gcd(a,2))",
     "Convert $y-({b})=({a}/2)(x+3)$ to $Ax+By=C$. Give integer $(A,B,C)$ with $A>0$ and no common factor.",
     "Multiply by $2$, expand, and collect terms to get ${a}x-2y=-3({a})-2({b})$. Divide all coefficients by the greatest common divisor of ${a}$ and $2$.",
     "Clear the denominator before collecting variable terms on one side."),
    ("point-slope-standard-form/kp2", [1,2,3,4], [-7,1,5], "(a/3,-b/3)",
     "Isolate $y$ in ${a}x-3y={b}$. Enter $(m,b)$ for the resulting $y=mx+b$.",
     "Subtract ${a}x$ to obtain $-3y={b}-{a}x$. Division by $-3$ gives slope ${a}/3$ and constant $-{b}/3$.",
     "Apply the same division to every term after isolating the vertical-variable term."),
    ("point-slope-standard-form/kp3", [-4,-1,2,5], [7,9,11],
     "((b-a)/gcd(b-a,2),-2/gcd(b-a,2),(b-3*a)/gcd(b-a,2))",
     "Find the line through $(1,{a})$ and $(3,{b})$ in $Ax+By=C$. Enter integer $(A,B,C)$ with positive $A$ and no common factor.",
     "The direction has run $2$ and rise ${b}-({a})$. Clear the denominator in point-slope form to obtain $({b}-({a}))x-2y={b}-3({a})$, then divide all coefficients by their common factor.",
     "Find a direction from the two points, clear the slope denominator, and reduce the coefficients."),
    ("parallel-perpendicular-lines/kp3", [-4,-2,2,4], [-5,1,6], "(3/a,b-6/a)",
     "Find the line perpendicular to ${a}x+3y=7$ through $(2,{b})$. Enter its slope-intercept coefficients $(m,b)$ in $y=mx+b$.",
     "The reference slope is $-{a}/3$, so its perpendicular has slope $3/({a})$. The point gives intercept ${b}-2(3/({a}))$.",
     "Isolate the vertical variable in the reference equation before taking the negative reciprocal."),
    ("linear-word-problems/kp2", [50,56,62,68], [74,80,86], "((b-a)/3,(5*a-2*b)/3)",
     "A club charges a joining fee plus a constant monthly rate. Total cost is EUR ${a}$ for $2$ months and EUR ${b}$ for $5$ months. For $C=mt+b$, enter $(m,b)$ in EUR/month and EUR.",
     "The extra $3$ months cost ${b}-({a})$, so divide that difference by $3$ for the monthly rate. Subtract two monthly payments from ${a}$ to recover the joining fee.",
     "Use the difference between the totals to separate the monthly rate from the fixed fee."),
    ("graphing-linear-equations/kp2", [2,4,6], [12,24,36,48], "(b/a,0,0,b/3)",
     "Graph ${a}x+3y={b}$ using intercepts. Enter the horizontal-axis point followed by the vertical-axis point as $(x_1,y_1,x_2,y_2)$.",
     "Set $y=0$ to solve ${a}x={b}$ for the horizontal intercept. Set $x=0$ to solve $3y={b}$ for the vertical intercept. Plot these two points and join them.",
     "Each axis crossing has one coordinate equal to zero."),
])


RECIPES.extend([
    ("constant-of-proportionality/kp2", [7,11,13,17], [2,3,5], "a/b",
     "A table has input row $x$: ${b}$, $2({b})$, $3({b})$ and corresponding output row $y$: ${a}$, $2({a})$, $3({a})$. Entries written as products denote their numerical values. Find the common constant in $y=kx$.",
     "Compare output/input in each column: ${a}/{b}$, $2({a})/(2({b}))$, and $3({a})/(3({b}))$. Cancelling the common factor in each ratio confirms one multiplier for all rows.",
     "Pair each output with the input in the same column before comparing ratios."),
    ("slope/kp3", [-4,-1,2,5], [7,9,11], "2*b-a",
     "Find $h$ so that $(1,{a})$, $(3,{b})$, and $(5,h)$ are collinear.",
     "Both horizontal gaps are $2$, so the vertical changes must match: $h-({b})={b}-({a})$. Add ${b}$ to recover the final height.",
     "Collinear points have the same slope between consecutive pairs."),
])


RECIPES.append(
    ("reading-slope-intercept-equations/kp3", [-6,-2,2,6], [4,8,16],
     "(a/gcd(a,b),b/gcd(a,b))",
     "For $y=({a}/{b})x+5$, report the signed integer rise and smallest positive integer run as $(r,s)$.",
     "The slope is the coefficient ${a}/{b}$. Divide its numerator and denominator by their greatest common divisor; keep the denominator positive so the move is rightward.",
     "Reduce the slope fraction before reading its numerator as rise and denominator as run.")
)


def premise(key, a, b):
    if key == "reading-slope-intercept-equations/kp3":
        return {"type":"rise_run", "slope":str(Q(a,b))}
    if key == "constant-of-proportionality/kp2":
        return {"type":"table_ratio", "rows":[[b,a],[2*b,2*a],[3*b,3*a]]}
    if key == "slope/kp3":
        return {"type":"collinear", "points":[[1,a],[3,b],[5,None]]}
    if key == "proportional-relationships/kp1":
        return {"type":"product", "factors":[a,b]}
    if key == "graphing-proportional-relationships/kp2":
        return {"type":"ratio", "input":b, "output":a}
    if key == "slope-from-a-graph/kp2":
        return {"type":"ratio", "input":b, "output":-a}
    if key == "point-slope-form/kp1":
        return {"type":"point_form", "point":[a,b], "slope":-2}
    if key == "point-slope-standard-form/kp1":
        return standard([[-3,b]], [2,a])
    if key == "point-slope-standard-form/kp2":
        return dict(line([]), abc=[a,-3,b])
    if key == "point-slope-standard-form/kp3":
        return standard([[1,a],[3,b]])
    if key == "parallel-perpendicular-lines/kp3":
        return {"type":"perpendicular", "points":[[2,b]], "reference":[3,-a]}
    if key == "linear-word-problems/kp2":
        return {"type":"model", "points":[[2,a],[5,b]]}
    if key == "graphing-linear-equations/kp2":
        return {"type":"intercepts", "abc":[a,3,b]}
    if key == "plotting-points/kp1":
        return {"type": "position", "moves": [[2,1],[a,0],[0,b]]}
    if key in {"constant-of-proportionality/kp1", "slope-from-a-graph/kp1", "slope/kp1"}:
        return {"type": "ratio", "input": b, "output": a}
    if key == "reading-slope-intercept-equations/kp1":
        return {"type": "coefficients", "m": a, "b": b}
    if key == "slope-intercept-form/kp1":
        return line([[2,b]], [1,a])
    if key == "slope-intercept-form/kp2":
        return line([[-1,a],[2,b]])
    if key == "slope-intercept-form/kp3":
        return line([[a,0],[0,b]])
    if key == "point-slope-form/kp3":
        return line([[-3,b]], [1,a])
    return {"type": "perpendicular", "points": [[2,b]], "reference": [1,a]}


def expected(key, a, b):
    if key == "reading-slope-intercept-equations/kp3":
        return f"({Q(a,b).numerator},{Q(a,b).denominator})"
    if key == "constant-of-proportionality/kp2":
        return str(Q(a,b))
    if key == "slope/kp3":
        return str(2*b-a)
    if key == "proportional-relationships/kp1":
        return str(a*b)
    if key == "graphing-proportional-relationships/kp2":
        return str(Q(a,b))
    if key == "slope-from-a-graph/kp2":
        return str(Q(-a,b))
    if key == "point-slope-form/kp1":
        return f"({-b},-2,{-a})"
    if key == "point-slope-standard-form/kp1":
        d = gcd(a,2)
        return f"({a//d},{-2//d},{(-3*a-2*b)//d})"
    if key == "point-slope-standard-form/kp2":
        return f"({Q(a,3)},{Q(-b,3)})"
    if key == "point-slope-standard-form/kp3":
        d = gcd(b-a,2)
        return f"({(b-a)//d},{-2//d},{(b-3*a)//d})"
    if key == "parallel-perpendicular-lines/kp3":
        return f"({Q(3,a)},{b-Q(6,a)})"
    if key == "linear-word-problems/kp2":
        return f"({Q(b-a,3)},{Q(5*a-2*b,3)})"
    if key == "graphing-linear-equations/kp2":
        return f"({Q(b,a)},0,0,{Q(b,3)})"
    if key == "plotting-points/kp1":
        return f"({a+2},{b+1})"
    if key in {"constant-of-proportionality/kp1", "slope-from-a-graph/kp1", "slope/kp1"}:
        return str(Q(a,b))
    if key == "reading-slope-intercept-equations/kp1":
        return f"({a},{b})"
    if key == "slope-intercept-form/kp1":
        m, c = a, b-2*a
    elif key == "slope-intercept-form/kp2":
        m, c = Q(b-a,3), Q(2*a+b,3)
    elif key == "slope-intercept-form/kp3":
        m, c = Q(-b,a), b
    elif key == "point-slope-form/kp3":
        m, c = a, 3*a+b
    else:
        m, c = Q(-1,a), b+Q(2,a)
    return f"({m},{c})"


def build():
    drafts, evidence = [], []
    for key, aa, bb, expr, statement, sketch, hint in RECIPES:
        samples, cases = [], []
        for a, b in itertools.product(aa, bb):
            bindings = dict(a=a, b=b)
            answer = expected(key, a, b)
            samples.append(dict(params=bindings, expected=answer))
            cases.append(dict(params=bindings, premise=premise(key,a,b)))
        contract, authored = AUTHORED[key]
        if authored[0]["premise"]["type"] in {"line", "perpendicular"} and "Enter" not in statement:
            statement += " Enter $(m,b)$ for $y=mx+b$."
        arguments = dict(answer_contract=contract, answer_expr=expr, statement=statement,
                         solution_sketch=sketch, hints=[hint], constraints=[], distractors=[],
                         params={"a": {"kind": "choice", "values": aa},
                                 "b": {"kind": "choice", "values": bb}}, samples=samples)
        drafts.append(dict(kp_id=key, kind="template", status="pending", arguments=arguments))
        evidence.append(dict(kp_id=key, authored=authored, cases=cases))
    return drafts, evidence


def patch_yaml():
    path = ROOT / "curriculum/foundations/04-linear-graphs.yaml"
    text = path.read_text()
    for key, (contract, tasks) in AUTHORED.items():
        topic, kp = key.split("/")
        start = text.index(f"  - id: {topic}\n")
        end = text.find("\n  - id:", start+1)
        end = len(text) if end == -1 else end
        section = text[start:end]
        pattern = rf"(      - id: {kp}\n.*?        exemplars:\n).*?(        constraints:)"
        rows = []
        for item in tasks:
            rows.extend(["          - problem: " + json.dumps(item["problem"]),
                         "            answer: " + json.dumps(item["answer"]),
                         "            answer_contract: " + json.dumps(contract),
                         "            solution_sketch: " + json.dumps(item["solution_sketch"])])
        section, count = re.subn(pattern, lambda m: m[1]+"\n".join(rows)+"\n"+m[2],
                                section, count=1, flags=re.S)
        assert count == 1, key
        text = text[:start] + section + text[end:]
    path.write_text(text)


def main():
    drafts, evidence = build()
    DEST.mkdir(exist_ok=True)
    for name, data in [("drafts.json", drafts), ("semantic-premises.json", evidence)]:
        (DEST/name).write_text(json.dumps(data, indent=2)+"\n")
    patch_yaml()


if __name__ == "__main__":
    main()
