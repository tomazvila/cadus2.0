"""Bounded U07 repairs; expected samples preserve the requested representation."""
import itertools
import re

import sympy as s

from templates import term


def recipe(key, statement, answer, sketch, axes, derived=None, contract=None):
    derived = derived or {}
    samples = []
    for values in itertools.product(*axes.values()):
        bound = dict(zip(axes, values))
        for name, formula in derived.items():
            bound[name] = int(s.sympify(formula).subs(bound))
        expected = re.sub(r"\b[a-z]+\b", lambda m: f"({bound[m[0]]})" if m[0] in bound else m[0], answer)
        samples.append({"params": bound, "expected": expected})
    params = {name: {"kind": "choice", "values": list(values)} for name, values in axes.items()}
    for name in derived:
        params[name] = {"kind": "choice", "values": sorted({r["params"][name] for r in samples})}
    arguments = dict(statement=statement, answer_expr=answer, solution_sketch=sketch,
                     hints=["Identify the structure first, then check each step by reversing it."],
                     params=params, samples=samples, distractors=[], constraints=[
                         {"op": "eq", "left": name, "right": term(s.sympify(formula))}
                         for name, formula in derived.items()])
    if contract:
        arguments["answer_contract"] = contract
    return dict(kp_id=key, kind="template", status="pending", arguments=arguments)


def factoring():
    r = recipe
    return [
        r("factoring-gcf/kp2", "Extract the negative GCF from $-{a}x^4+{b}x^2-{a}x$.",
          "-a*x*(x**3-c*x+1)",
          "The coefficient GCF is ${a}$ and the lowest power is $x$. Division by $-{a}x$ leaves $x^3$, $-{c}x$, and $1$; retain that final unit term.",
          {"a": [2, 3, 5], "c": [2, 3, 4, 5]}, {"b": "a*c"}),
        r("difference-of-squares/kp2", "Factor ${a}x^4-{b}$ completely over the integers.",
          "(sqrt(a)*x**2-sqrt(b))*(sqrt(a)*x**2+sqrt(b))",
          r"The square roots are $\sqrt{{{a}}}x^2$ and $\sqrt{{{b}}}$. Use their difference and sum. Neither resulting quadratic has rational roots, so the integer factorization is complete.",
          {"a": [4, 9, 25], "b": [49, 121, 169, 289]}),
        r("difference-of-squares/kp3", "Factor ${g}x^2-{b}$ completely over the integers.",
          "g*(x-sqrt(b/g))*(x+sqrt(b/g))",
          r"Extract the coefficient GCF ${g}$, leaving $x^2-{b}/{g}$. The constant is a square; its root is ${d}$, giving the two conjugate linear factors.",
          {"g": [5, 7, 11], "d": [2, 4, 5, 6]}, {"b": "g*d**2"}),
        r("perfect-square-trinomials/kp3", "Write ${q}x^2-{b}x+{c}$ as one squared binomial.",
          "(a*x-d)**2",
          "The end terms are $({a}x)^2$ and ${d}^2$. Twice their square-root product is ${b}x$. The negative middle term selects $({a}x-{d})^2$.",
          {"a": [3, 5, 7], "d": [2, 4, 8, 11]}, {"q": "a**2", "b": "2*a*d", "c": "d**2"}),
        r("quadratics-in-form/kp2", "Factor $x^4-{b}x^2+{c}$ completely over the integers.",
          "(x-a)*(x+a)*(x-d)*(x+d)",
          "Set $u=x^2$. The constants $-{a}^2$ and $-{d}^2$ sum to $-{b}$ and multiply to ${c}$. Thus $(u-{a}^2)(u-{d}^2)$ splits into four linear factors after restoring $x^2$.",
          {"a": [2, 3, 4], "d": [5, 6, 7, 8]}, {"b": "a**2+d**2", "c": "a**2*d**2"}),
        r("choosing-factoring-strategy/kp2", "Factor ${g}x^3+{b}x^2+{c}x$ completely over the integers.",
          "g*x*(x+d)**2",
          "The first move is to extract ${g}x$. The remaining trinomial is $x^2+2({d})x+{d}^2$, a perfect square. Keep the GCF outside the squared binomial.",
          {"g": [2, 3, 5], "d": [3, 4, 6, 7]}, {"b": "2*g*d", "c": "g*d**2"}),
        r("choosing-factoring-strategy/kp3", "Factor $x^3+{a}x^2-{b}x-{c}$ completely over the integers.",
          "(x+a)*(x-d)*(x+d)",
          "Group to obtain $x^2(x+{a})-{b}(x+{a})$. Extract $x+{a}$, then split $x^2-{b}$ into $x-{d}$ and $x+{d}$. All three remaining factors are linear.",
          {"a": [5, 6, 7], "d": [4, 8, 9, 10]}, {"b": "d**2", "c": "a*d**2"}),
    ]


def root_count():
    row = recipe("quadratic-formula/kp3",
                 "How many distinct real solutions does $3x^2+({b})x+{c}=0$ have? Use the quadratic formula's discriminant.",
                 "1+(b**2-12*c)/max(1,abs(b**2-12*c))",
                 "Compute $D=({b})^2-12({c})$. In the quadratic formula, positive $D$ gives two real square-root branches, zero merges them into one, and negative $D$ gives no real square root.",
                 {"b": [-6, -12, -24, -30], "offset": [-1, 0, 1]}, {"c": "b**2/12+offset"})
    # The offset constructs the coefficients; D is also useful in the worked sketch.
    row["arguments"]["solution_sketch"] += " Here $D=-12({offset})$."
    for sample in row["arguments"]["samples"]:
        sample["expected"] = str(1 - sample["params"]["offset"])
    return row


def replacements():
    return {row["kp_id"]: row for row in [*factoring(), root_count()]}
