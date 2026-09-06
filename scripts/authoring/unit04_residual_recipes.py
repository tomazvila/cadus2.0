"""Build Unit04-only pending recipes and premises; preserve unrelated YAML bytes."""
import itertools
import json
import re
from fractions import Fraction as Q
from pathlib import Path

from unit04_residual_data import AUTHORED, line

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


def premise(key, a, b):
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
        if authored[0]["premise"]["type"] in {"line", "perpendicular"}:
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
