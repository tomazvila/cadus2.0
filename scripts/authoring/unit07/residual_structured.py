"""Explicit pending response contracts for mixed-shape authored knowledge points."""
from residual_recipes import recipe

EXACT = {"kind": "exact"}
POINT = {"kind": "coordinates", "arity": 2}


def multipart(**parts):
    return {"kind": "multipart", "parts": [{"name": name, "contract": contract}
                                           for name, contract in parts.items()]}


def named_samples(row, names, answer):
    for sample in row["arguments"]["samples"]:
        values = answer(sample["params"])
        sample["expected"] = "; ".join(f"{name} = {value}" for name, value in zip(names, values))
    return row


def graphing():
    axis = recipe("parabola-vertex-form/kp3",
                  "For $y=-2(x-({a}))^2+{b}$, give the symmetry axis as x and the ordinate at x=0 as y.",
                  "multipart(a,b-2*a**2)",
                  "The squared term is centered at $x={a}$, which gives the axis. At zero input, $y=-2(0-{a})^2+{b}={b}-2({a})^2$.",
                  {"a": [-3, -2, -1, 1], "b": [6, 7, 9]}, contract=multipart(x=EXACT, y=EXACT))
    named_samples(axis, ["x", "y"], lambda p: (p["a"], p["b"]-2*p["a"]**2))
    vertex = recipe("quadratic-graphs-vertex/kp2",
                    "For $y=x^2+{a}x+{b}$, give vertex and y_intercept, each as a coordinate pair.",
                    "multipart((-a/2,b-a**2/4),(0,b))",
                    "The vertex input is $-{a}/2$. Substituting gives ordinate ${b}-({a})^2/4$. At $x=0$, the constant ${b}$ gives the vertical-axis intercept. The polynomial also factors as $(x+3)(x+({a}-3))$.",
                    {"a": list(range(9, 21))}, {"b": "3*(a-3)"},
                    contract=multipart(vertex=POINT, y_intercept=POINT))
    named_samples(vertex, ["vertex", "y_intercept"],
                  lambda p: (f"(-{p['a']}/2, {p['b']}-{p['a']}**2/4)", f"(0, {p['b']})"))
    return [axis, vertex]


def applications():
    area = recipe("quadratic-applications/kp1",
                  "A rectangle has area ${a}$ square units and length five units more than its width. Find its positive width.",
                  "(-5+sqrt(25+4*a))/2",
                  "Let width be $w$, so $w(w+5)={a}$. Rearrange to $w^2+5w-{a}=0$ and factor as $(w-{d})(w+{d}+5)=0$. The negative root cannot be a length; the width is ${d}$.",
                  {"d": list(range(2, 14))}, {"a": "d*(d+5)"}, contract=EXACT)
    for sample in area["arguments"]["samples"]:
        sample["expected"] = str(sample["params"]["d"])
    height = recipe("quadratic-applications/kp2",
                    "A ball's height is $h(t)=-5t^2+{b}t$ metres. Find its maximum height, including units.",
                    "b**2/20",
                    "The leading coefficient is negative, so the vertex is a maximum. Its time is $t={b}/10$. Substitution gives $-5({b}/10)^2+{b}({b}/10)=({b})^2/20$ metres.",
                    {"b": list(range(15, 135, 10))},
                    contract={"kind": "unit", "quantity": "length", "unit": "m"})
    for sample in height["arguments"]["samples"]:
        b = sample["params"]["b"]
        sample["expected"] = f"{b*b}/20 m"
    return [area, height]


def replacements():
    return {row["kp_id"]: row for row in [*graphing(), *applications()]}
