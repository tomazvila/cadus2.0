"""Declarative arithmetic only. No eval, executable input, or model-selected Lean."""
from dataclasses import dataclass
from fractions import Fraction
from itertools import product
import re

class Unsupported(ValueError):
    pass

@dataclass(frozen=True)
class Expr:
    op: str
    value: object = None
    args: tuple = ()

def bounded(x):
    if max(x.numerator.bit_length(), x.denominator.bit_length()) > 2048:
        raise Unsupported("Exact number exceeds 2048 bits.")
    return x

def names_in(e):
    return {e.value} if e.op == "var" else set().union(*(names_in(a) for a in e.args))

def evaluate(e, values):
    if e.op == "num":
        return e.value
    if e.op == "var":
        return Fraction(values[e.value])
    a = [evaluate(x, values) for x in e.args]
    if e.op == "neg":
        result = -a[0]
    elif e.op == "+":
        result = a[0] + a[1]
    elif e.op == "-":
        result = a[0] - a[1]
    elif e.op == "*":
        result = a[0] * a[1]
    elif e.op == "/":
        if not a[1]:
            raise Unsupported("Division by zero.")
        result = a[0] / a[1]
    elif e.op == "^":
        result = a[0] ** e.value
    else:
        raise Unsupported("Unknown arithmetic node.")
    return bounded(result)

TOKEN = re.compile(r"\s*(?:(\d+(?:\.\d+)?)|([A-Za-z])|(\*\*|[+\-*/^()]))")

class Parser:
    def __init__(self, text, names):
        if not isinstance(text, str) or not text.strip() or len(text) > 512:
            raise Unsupported("Expression must contain 1..512 characters.")
        self.tokens, pos = [], 0
        while pos < len(text) and text[pos:].strip():
            m = TOKEN.match(text, pos)
            if not m:
                raise Unsupported("Unsupported arithmetic notation.")
            self.tokens.append(next(x for x in m.groups() if x is not None))
            pos = m.end()
        if len(self.tokens) > 256:
            raise Unsupported("Too many tokens.")
        self.pos, self.nodes, self.names = 0, 0, set(names)

    def peek(self):
        return self.tokens[self.pos] if self.pos < len(self.tokens) else None

    def take(self):
        x = self.peek()
        if x is None:
            raise Unsupported("Missing arithmetic value.")
        self.pos += 1
        return x

    def node(self, op, value=None, args=()):
        self.nodes += 1
        if self.nodes > 128:
            raise Unsupported("Too many arithmetic nodes.")
        return Expr(op, value, args)

    def sum(self, depth=0):
        if depth > 24:
            raise Unsupported("Expression nesting exceeds 24.")
        e = self.product(depth)
        while self.peek() in ("+", "-"):
            op = self.take()
            e = self.node(op, args=(e, self.product(depth)))
        return e

    def product(self, depth):
        e = self.unary(depth)
        while self.peek() in ("*", "/"):
            op = self.take()
            e = self.node(op, args=(e, self.unary(depth)))
        return e

    def unary(self, depth):
        if depth > 24:
            raise Unsupported("Expression nesting exceeds 24.")
        if self.peek() in ("+", "-"):
            op = self.take()
            e = self.unary(depth + 1)
            return e if op == "+" else self.node("neg", args=(e,))
        e = self.atom(depth)
        if self.peek() in ("^", "**"):
            self.take()
            exp = self.unary(depth + 1)
            if names_in(exp):
                raise Unsupported("Variable exponent.")
            n = evaluate(exp, {})
            if n.denominator != 1 or not 0 <= n <= 12:
                raise Unsupported("Exponents must be integers 0..12.")
            e = self.node("^", int(n), (e,))
        return e

    def atom(self, depth):
        t = self.take()
        if t == "(":
            e = self.sum(depth + 1)
            if self.take() != ")":
                raise Unsupported("Unbalanced parentheses.")
            return e
        if re.fullmatch(r"\d+(?:\.\d+)?", t):
            n = bounded(Fraction(t))
            if n > 1_000_000_000:
                raise Unsupported("Literal exceeds 1e9.")
            return self.node("num", n)
        if t in self.names:
            return self.node("var", t)
        raise Unsupported("Only declared single-letter variables are accepted.")

def check_divisors(e):
    for a in e.args:
        check_divisors(a)
    if e.op == "/" and (names_in(e.args[1]) or evaluate(e.args[1], {}) == 0):
        raise Unsupported("Denominators must be constant and nonzero.")

def parse_expression(text, names=()):
    p = Parser(text, names)
    e = p.sum()
    if p.peek() is not None:
        raise Unsupported("Implicit multiplication or trailing text.")
    check_divisors(e)
    return e

def rational(n):
    n = Fraction(n)
    a = str(n.numerator) if n.numerator >= 0 else "(-" + str(-n.numerator) + ")"
    return "(" + a + " : Rat)" if n.denominator == 1 else "((" + a + " : Rat) / " + str(n.denominator) + ")"

def render(e):
    if e.op == "num":
        return rational(e.value)
    if e.op == "var":
        return e.value
    if e.op == "neg":
        return "(-" + render(e.args[0]) + ")"
    if e.op == "^":
        return "(" + render(e.args[0]) + " ^ " + str(e.value) + ")"
    return "(" + render(e.args[0]) + " " + e.op + " " + render(e.args[1]) + ")"

@dataclass(frozen=True)
class FactorAnswer:
    members: tuple
    pairs: tuple

def strip_outer(text, left, right):
    if not (text.startswith(left) and text.endswith(right)):
        return text
    depth = 0
    for i, c in enumerate(text):
        depth += (c == left) - (c == right)
        if depth == 0 and i < len(text) - 1:
            return text
    return text[1:-1].strip() if depth == 0 else text

def parse_factors(text):
    if not isinstance(text, str) or not text.strip() or len(text) > 2048:
        raise Unsupported("Factor answer must contain 1..2048 characters.")
    text = text.strip()
    if text.startswith("$") and text.endswith("$"):
        text = text[1:-1].strip()
    text = text.replace("\\times", "*").replace("\u00d7", "*").replace("\u00b7", "*")
    text = re.sub(r"\btimes\b", "*", text, flags=re.I)
    text = re.sub(r"\band\b", ",", text, flags=re.I)
    for left, right in (("[", "]"), ("{", "}")):
        text = strip_outer(text, left, right)
    if text.count("(") == 1 and text.count(")") == 1:
        text = strip_outer(text, "(", ")")
    chunks, start, depth = [], 0, 0
    for i, c in enumerate(text):
        depth += (c == "(") - (c == ")")
        if depth < 0 or depth > 1:
            raise Unsupported("Unsupported factor grouping.")
        if depth == 0 and c in ",;\n":
            chunks.append(text[start:i].strip())
            start = i + 1
    if depth:
        raise Unsupported("Unbalanced factor grouping.")
    chunks.append(text[start:].strip())
    if any(not c for c in chunks):
        raise Unsupported("Empty factor-list member.")
    members, pairs = [], []
    for chunk in chunks:
        m = re.fullmatch(r"\(?\s*(\d+)\s*[xX*]\s*(\d+)\s*\)?", chunk)
        m = m or re.fullmatch(r"\(\s*(\d+)\s*,\s*(\d+)\s*\)", chunk)
        if m:
            a, b = map(int, m.groups())
            pairs.append((a, b))
            members.extend((a, b))
        elif re.fullmatch(r"\d+(?:\s+\d+)*", chunk):
            members.extend(map(int, chunk.split()))
        else:
            raise Unsupported("Use integer lists or comma-separated integer pairs.")
        if len(members) > 128 or any(n > 1_000_000_000 for n in members):
            raise Unsupported("Factor answer exceeds its resource bound.")
    return FactorAnswer(tuple(members), tuple(pairs))

@dataclass(frozen=True)
class Problem:
    kind: str
    target: int = 0
    expression: object = None
    variables: tuple = ()

def prepare_problem(data):
    if not isinstance(data, dict):
        raise Unsupported("problem must be an object.")
    kind = data.get("kind")
    if kind == "factor_list":
        if set(data) != {"kind", "target"} or type(data["target"]) is not int or not 1 <= data["target"] <= 4096:
            raise Unsupported("Factor target must be an integer 1..4096, with no extra fields.")
        return Problem(kind, target=data["target"])
    if kind in ("numeric_expression", "polynomial_identity"):
        fields = {"kind", "expression"} | ({"variables"} if kind == "polynomial_identity" else set())
        if set(data) != fields:
            raise Unsupported("Unexpected arithmetic fields.")
        names = data.get("variables", [])
        if not isinstance(names, list) or len(names) > 4 or any(not isinstance(n, str) or not re.fullmatch("[A-Za-z]", n) for n in names):
            raise Unsupported("Declare at most four single ASCII-letter variables.")
        if len(names) != len(set(names)):
            raise Unsupported("Duplicate variable declaration.")
        return Problem(kind, expression=parse_expression(data["expression"], names), variables=tuple(names))
    raise Unsupported("Unsupported problem kind.")

@dataclass(frozen=True)
class Claim:
    statement: str
    tactic: str
    status: str
    imports: tuple
    interpretation: dict

def prepare_claim(problem, answer):
    if problem.kind == "factor_list":
        a = parse_factors(answer)
        submitted = sorted(set(a.members))
        expected = {n for n in range(1, problem.target + 1) if problem.target % n == 0}
        correct = set(submitted) == expected and all(x > 0 and y > 0 and x*y == problem.target for x,y in a.pairs)
        clauses = ["(([" + ", ".join(map(str, submitted)) + "] : List Nat).toFinset = Nat.divisors " + str(problem.target) + ")"]
        for x,y in a.pairs:
            clauses.append("((0 < (" + str(x) + " : Nat)) /\\ (0 < (" + str(y) + " : Nat)) /\\ (" + str(x) + " * " + str(y) + " = " + str(problem.target) + "))")
        prop = " /\\ ".join(clauses)
        return Claim(prop if correct else "Not (" + prop + ")", "decide +kernel",
                     "proved" if correct else "disproved", ("Mathlib.NumberTheory.Divisors",),
                     {"members":list(a.members), "pairs":[list(p) for p in a.pairs],
                      "semantics":"complete positive divisor set and validity of every written pair"})
    answer_expr = parse_expression(answer, problem.variables)
    left, right = render(problem.expression), render(answer_expr)
    equality = left + " = " + right
    imports = ("Mathlib.Tactic.NormNum",)
    if not problem.variables:
        correct = evaluate(problem.expression,{}) == evaluate(answer_expr,{})
        return Claim(equality if correct else "Not (" + equality + ")", "norm_num",
                     "proved" if correct else "disproved", imports,
                     {"baseline":left,"answer":right,"domain":"exact rational arithmetic"})
    names = " ".join(problem.variables)
    universal = "forall (" + names + " : Rat), " + equality
    imports += ("Mathlib.Tactic.Ring",)
    for values in product((-2,-1,0,1,2), repeat=len(problem.variables)):
        witness = dict(zip(problem.variables, values))
        if evaluate(problem.expression,witness) != evaluate(answer_expr,witness):
            tactic = "intro h\n  have h_at := h " + " ".join(rational(v) for v in values) + "\n  norm_num at h_at"
            return Claim("Not (" + universal + ")", tactic, "disproved", imports,
                         {"baseline":left,"answer":right,"domain":"all rational assignments","counterexample":witness})
    return Claim(universal, "intro " + names + "\n  ring", "proved", imports,
                 {"baseline":left,"answer":right,"domain":"all rational assignments",
                  "note":"Finite probing chooses a tactic only; failed proof remains unresolved."})
