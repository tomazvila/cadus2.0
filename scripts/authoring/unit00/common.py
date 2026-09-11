"""Closed unit00 recipe construction; samples use an independent Python oracle."""
import itertools
import math
import re
import ast
from fractions import Fraction
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from exact_arithmetic import binary


def lit(value):
    return {"lit": value}


def op(name, *terms):
    return {name: list(terms)}


def condition(name, left, right):
    return {"op": name, "left": left, "right": right}


def term(value, bindings):
    if isinstance(value, str):
        return bindings[value]
    name, args = next(iter(value.items()))
    if name == "lit":
        return args
    values = [term(a, bindings) for a in args]
    return {"add": lambda: sum(values), "sub": lambda: values[0]-values[1],
            "mul": lambda: math.prod(values), "mod": lambda: values[0]%values[1]}[name]()


def holds(rule, bindings):
    a, b = term(rule["left"], bindings), term(rule["right"], bindings)
    return {"eq": lambda: a == b, "ne": lambda: a != b, "le": lambda: a <= b,
            "lt": lambda: a < b, "ge": lambda: a >= b, "gt": lambda: a > b,
            "divides": lambda: b % a == 0, "coprime": lambda: math.gcd(a,b) == 1}[rule["op"]]()


def normalized(text):
    text = text.lower().replace("evaluate", "compute")
    text = text.replace(r"\times", "*").replace(r"\div", "/")
    text = text.replace("{,}", "").replace("{", "").replace("}", "")
    return re.sub(r"[\s$]", "", text).rstrip(".")


def semantic_key(problem, answer):
    numbers=tuple(sorted(re.findall(r"\d+",problem.replace("{,}",""))))
    answer=re.sub(r"[\s$()]","",str(answer))
    answer=re.sub(r"(?<=\d)x(?=\d)","*",answer)
    try:
        answer=str(constant(ast.parse(answer.replace('^','**'),mode='eval').body))
    except (ValueError,SyntaxError,TypeError,ZeroDivisionError):
        pass
    return numbers,answer


def constant(node):
    if isinstance(node,ast.Constant) and isinstance(node.value,(int,float)):
        return Fraction(str(node.value))
    if isinstance(node,ast.Tuple):
        return ','.join(str(constant(n)) for n in node.elts)
    if isinstance(node,ast.BinOp):
        a,b=constant(node.left),constant(node.right)
        if isinstance(node.op,ast.Pow) and b.denominator==1 and 0<=b<=1000:
            return a**int(b)
        return binary(node.op,a,b)
    raise ValueError('outside closed arithmetic')


def recipe(key, statement, expr, domains, oracle, sketch, hint, constraints=()):
    return dict(kp_id=key, statement=statement, answer_expr=expr, domains=domains,
                oracle=oracle, solution_sketch=sketch, hints=[hint], constraints=list(constraints))


def arguments(item, forbidden):
    domains = {k: sorted(set(v)) for k, v in item["domains"].items()}
    constraints = list(item["constraints"])
    texts={normalized(e['problem']) for e in forbidden}
    calculations={semantic_key(e['problem'],e['answer']) for e in forbidden}
    samples = []
    for values in itertools.product(*domains.values()):
        bindings = dict(zip(domains, values))
        if not all(holds(c, bindings) for c in constraints):
            continue
        problem = item["statement"].format(**bindings)
        expected = item["oracle"](**bindings)
        if normalized(problem) in texts or semantic_key(problem,expected) in calculations:
            # Exclude the exact tuple without enlarging the mathematical domain.
            distance = op("add", *(op("mul", op("sub", k, lit(v)),
                          op("sub", k, lit(v))) for k, v in bindings.items()))
            if len(bindings) == 1:
                k, v = next(iter(bindings.items()))
                distance = op("sub", k, lit(v))
            constraints.append(condition("ne", distance, lit(0)))
            continue
        samples.append({"params": bindings, "expected": str(expected)})
    fields = ("statement", "answer_expr", "solution_sketch", "hints")
    return {**{k: item[k] for k in fields}, "params": {
        k: ({"kind": "int", "low": v[0], "high": v[-1]} if len(v) > 24
            and v == list(range(v[0], v[-1]+1)) else {"kind": "choice", "values": v})
        for k, v in domains.items()},
        "constraints": constraints, "samples": samples, "distractors": []}
