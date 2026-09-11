"""Lossless coefficient/vertex-parameter responses for polynomial tasks."""
from residual_recipes import recipe
from residual_structured import EXACT, multipart, named_samples


def root_equations():
    simple = recipe("writing-quadratics-from-roots/kp1",
                    "A monic quadratic has roots ${a}$ and $-{b}$. Write its equation as $Ax^2+Bx+C=0$ by returning the coefficient tuple (A,B,C).",
                    "(1,b-a,-a*b)",
                    "The zero equation is $(x-{a})(x+{b})=0$. Expansion gives $x^2+({b}-{a})x-{a}({b})=0$. Read all three coefficients in descending powers.",
                    {"a": [6, 7, 8, 9], "b": [4, 6, 8]})
    repeated = recipe("writing-quadratics-from-roots/kp2",
                      "A monic quadratic has root list $({a},{b})$, counting multiplicity. Write its equation as $Ax^2+Bx+C=0$ by returning (A,B,C).",
                      "(1,-a-b,a*b)",
                      "Use $(x-{a})(x-{b})=0$. The linear coefficient is the negative root sum and the constant is their product. A zero root leaves a factor $x$; equal roots give a squared binomial.",
                      {"a": [2, 3, 6, 7, 8, 9], "b": [0, 2, 3, 6, 7, 8, 9]})
    repeated["arguments"]["constraints"] = [{"op": "eq", "left": {
        "mul": ["b", {"add": ["b", {"mul": [{"lit": -1}, "a"]}]}]}, "right": {"lit": 0}}]
    repeated["arguments"]["samples"] = [sample for sample in repeated["arguments"]["samples"]
                                          if sample["params"]["b"] in (0, sample["params"]["a"])]
    fractional = recipe("writing-quadratics-from-roots/kp3",
                        "A quadratic has roots $1/{a}$ and $-{b}$. Write $Ax^2+Bx+C=0$ with the smallest positive integer A by returning (A,B,C).",
                        "(a,a*b-1,-b)",
                        "Clearing the fractional root gives ${a}x-1$; the other factor is $x+{b}$. Their product has coefficients ${a}$, ${a}({b})-1$, and $-{b}$. These have no common integer factor, so A is minimal.",
                        {"a": [2, 3, 4], "b": [4, 5, 6, 7]})
    return [simple, repeated, fractional]


def square_completion():
    completion = recipe("completing-the-square/kp1",
                        "Complete $x^2+({a})x$ to $x^2+({a})x+c=(x+h)^2$. Return constant = c and shift = h.",
                        "multipart(a**2/4,a/2)",
                        "Expanding $(x+h)^2$ gives $x^2+2hx+h^2$. Match $2h={a}$, so $h={a}/2$ and the added constant is $c=({a}/2)^2$.",
                        {"a": [-16, -14, -12, -10, -8, -2, 2, 4, 6, 12, 14, 16]},
                        contract=multipart(constant=EXACT, shift=EXACT))
    named_samples(completion, ["constant", "shift"], lambda p: (p["a"]**2//4, p["a"]//2))
    coefficients = recipe("applying-the-quadratic-formula/kp1",
                          "Rearrange ${a}x^2+{c}={b}x$ into $Ax^2+Bx+C=0$ with every term on the left. Return (A,B,C).",
                          "(a,-b,c)",
                          "Subtract ${b}x$ from both sides to get ${a}x^2-{b}x+{c}=0$. The linear coefficient changes sign; read the quadratic, linear, and constant coefficients in that order.",
                          {"a": [2, 3, 5], "b": [4, 7], "c": [-2, 6]})
    return [completion, coefficients]


def vertex_forms():
    monic = recipe("converting-to-vertex-form/kp1",
                   "Complete the square for $y=x^2+({b})x+{c}$. Express it as $y=(x-h)^2+k$ by returning (h,k).",
                   "(-b/2,c-b**2/4)",
                   "Add and subtract $({b}/2)^2$. This gives $y=(x+{b}/2)^2+{c}-({b}/2)^2$. Comparing with $(x-h)^2+k$, the shift h is $-{b}/2$ and k is the remaining constant.",
                   {"b": [-6, -2, 4, 8], "c": [2, 6, 9]})
    nonmonic = recipe("converting-to-vertex-form/kp2",
                      "Complete the square for $y=({a})x^2+{b}x+5$. Express it as $y=A(x-h)^2+k$ by returning (A,h,k).",
                      "(a,-b/(2*a),5-b**2/(4*a))",
                      "Factor ${a}$ out of the variable terms. Half the inner linear coefficient is ${b}/(2({a}))$. Complete the square inside and compensate outside by subtracting $({b})^2/(4({a}))$. The outside constant is $5-({b})^2/(4({a}))$.",
                      {"a": [-2, 2, 3], "b": [8, 12, 16, 20]})
    interpretation = recipe("converting-to-vertex-form/kp3",
                           "Complete the square for $y={a}x^2+{b}x+7$ into $y=A(x-h)^2+k$. Return vertex_parameters = (A,h,k) and minimum = the minimum value of y.",
                           "multipart((a,-b/(2*a),7-b**2/(4*a)),7-b**2/(4*a))",
                           "After factoring ${a}$ out of the variable terms, complete the inner square using ${b}/(2({a}))$. The compensation is $({b})^2/(4({a}))$, giving k equal to $7-({b})^2/(4({a}))$. The positive leading coefficient makes this outside constant the minimum.",
                           {"a": [2, 3, 4], "b": [8, 12, 16, 20]},
                           contract=multipart(vertex_parameters=EXACT, minimum=EXACT))
    named_samples(interpretation, ["vertex_parameters", "minimum"],
                  lambda p: (f"({p['a']}, -{p['b']}/(2*{p['a']}), 7-{p['b']}**2/(4*{p['a']}))",
                             f"7-{p['b']}**2/(4*{p['a']})"))
    return [monic, nonmonic, interpretation]


def replacements():
    return {row["kp_id"]: row for row in [*root_equations(), *square_completion(), *vertex_forms()]}
