"""Pure rendering/arithmetic helpers for the `integers-negatives` recipes.

Every function here computes its returned answer from real Python
arithmetic (`int`/`Fraction`), never from a hand-typed literal, and renders
the matching LaTeX problem text and a truthful solution sketch from the same
computed values. None of these functions know a specific knowledge point's
own constraint; `apply_integers_negatives_recipes.py` supplies the operands
that satisfy each KP's constraint and names which helper fits its shape.
"""
from __future__ import annotations

from fractions import Fraction

from foundations_curriculum_patch import NewExemplar
from integers_negatives_contracts import ASCENDING_CHAIN, EXACT, LABEL_LT_GT, LABEL_TRUE_FALSE


def _signed(n: int) -> str:
    """A bare operand's own text: never parenthesized (used as the first term)."""
    return str(n)


def _after_op(n: int) -> str:
    """An operand's text once it follows a written `+`, `-`, `\\times` or `\\div`."""
    return f"({n})" if n < 0 else str(n)


def _exact(problem: str, answer: str, sketch: str, contract: str | None = None) -> NewExemplar:
    if contract is None:
        return NewExemplar(problem, answer, sketch, with_contract=False)
    return NewExemplar(problem, answer, sketch, with_contract=True, contract_override=contract)


def _add(a: int, b: int) -> NewExemplar:
    """`Compute $a + b$.` with the real sign rule stated in the sketch."""
    value = a + b
    problem = f"Compute ${_signed(a)} + {_after_op(b)}$."
    if (a < 0) == (b < 0):
        sketch = f"Same signs: ${abs(a)} + {abs(b)} = {abs(value)}$, keep the sign: ${value}$."
    elif abs(a) == abs(b):
        sketch = f"${a}$ and ${b}$ are opposites: opposites sum to $0$."
    else:
        bigger, smaller = (abs(a), abs(b)) if abs(a) >= abs(b) else (abs(b), abs(a))
        sign = "-" if value < 0 else ""
        sketch = f"Different signs: ${bigger} - {smaller} = {abs(value)}$, keep the sign of the larger absolute value: ${sign}{abs(value)}$."
    return _exact(problem, str(value), sketch)


def _add_three(a: int, b: int, c: int, *, note: str) -> NewExemplar:
    subtotal = a + b
    value = subtotal + c
    problem = f"Compute ${_signed(a)} + {_after_op(b)} + {_after_op(c)}$."
    sketch = f"${a} + {'(' + str(b) + ')' if b < 0 else b} = {subtotal}$; ${subtotal} + {'(' + str(c) + ')' if c < 0 else c} = {value}$. {note}"
    return _exact(problem, str(value), sketch)


def _sub(a: int, b: int) -> NewExemplar:
    """`Compute $a - b$.`, rewritten as adding the opposite (b may be negative)."""
    value = a - b
    problem = f"Compute ${_signed(a)} - {_after_op(b)}$."
    opp = -b
    sketch = f"${a} + {_after_op(opp)} = {value}$."
    return _exact(problem, str(value), sketch)


def _mixed_two_term(op: str, a: int, b: int) -> NewExemplar:
    """One `+` or `-` term mixing signs, for `integer-addition-subtraction/kp1`."""
    if op == "+":
        return _add(a, b)
    return _sub(a, b)


def _three_term_chain(a: int, terms: list[tuple[str, int]]) -> NewExemplar:
    """`a <op1> t1 <op2> t2 ...` evaluated strictly left to right."""
    pieces = [_signed(a)]
    running = a
    steps = []
    for op, term in terms:
        signed_term = _after_op(term) if op == "+" else _after_op(term)
        pieces.append(f"{op} {signed_term}")
        next_running = running + term if op == "+" else running - term
        steps.append(f"${running} {op} {signed_term} = {next_running}$")
        running = next_running
    problem = f"Compute ${' '.join(pieces)}$."
    sketch = "; then ".join(steps) + "."
    return _exact(problem, str(running), sketch)


def _missing_add(k: int, target: int) -> NewExemplar:
    """Solve `\\square + (k) = target`."""
    missing = target - k
    problem = f"What number makes $\\square + {_after_op(k)} = {target}$ true?"
    sketch = f"$\\square = {target} - {_after_op(k)} = {missing}$; check: ${missing} + {_after_op(k)} = {target}$."
    return _exact(problem, str(missing), sketch)


def _missing_sub(a: int, target: int) -> NewExemplar:
    """Solve `a - \\square = target`."""
    missing = a - target
    problem = f"What number makes ${a} - \\square = {target}$ true?"
    sketch = f"$\\square = {a} - {_after_op(target)} = {missing}$; check: ${a} - {_after_op(missing)} = {target}$."
    return _exact(problem, str(missing), sketch)


def _product(factors: list[int]) -> NewExemplar:
    value = 1
    for f in factors:
        value *= f
    negatives = sum(1 for f in factors if f < 0)
    parity = "even" if negatives % 2 == 0 else "odd"
    sign_word = "positive" if value >= 0 else "negative"
    text = r" \times ".join(_after_op(f) if i else _signed(f) for i, f in enumerate(factors))
    problem = f"Compute ${text}$."
    magnitudes = " \\times ".join(str(abs(f)) for f in factors)
    sketch = f"{negatives} negative factor{'s' if negatives != 1 else ''} ({parity}): the product is {sign_word}; ${magnitudes} = {abs(value)}$."
    return _exact(problem, str(value), sketch)


def _muldiv_chain(a: int, terms: list[tuple[str, int]]) -> NewExemplar:
    """`a <*or/> t1 <*or/> t2` evaluated strictly left to right, exact only."""
    pieces = [_signed(a)]
    running = Fraction(a)
    steps = []
    for op, term in terms:
        symbol = r"\times" if op == "*" else r"\div"
        pieces.append(f"{symbol} {_after_op(term)}")
        next_running = running * term if op == "*" else running / term
        assert next_running.denominator == 1, "the KP promises exact quotients"
        steps.append(f"${running} {symbol} {_after_op(term)} = {next_running}$")
        running = next_running
    problem = f"Compute ${' '.join(pieces)}$."
    sketch = "; ".join(steps) + "."
    return _exact(problem, str(running), sketch)


def _neg_base_power(base: int, exp: int) -> NewExemplar:
    """`(-base)^exp`, base written positive, the sign carried by the parens."""
    value = (-base) ** exp
    problem = f"Compute $(-{base})^{exp}$."
    factors = " \\times ".join([f"(-{base})"] * exp)
    parity = "even" if exp % 2 == 0 else "odd"
    sketch = f"${factors} = {value}$; an {parity} number of negative factors gives a {'positive' if value >= 0 else 'negative'} result."
    return _exact(problem, str(value), sketch)


def _neg_pow_no_parens(base: int, exp: int) -> NewExemplar:
    """`-base^exp`: the exponent binds before the leading minus."""
    powered = base**exp
    value = -powered
    problem = f"Compute $-{base}^{exp}$."
    sketch = f"The exponent binds first: $-({base}^{exp}) = -({powered}) = {value}$."
    return _exact(problem, str(value), sketch)


def _decimal_add(a: Fraction, b: Fraction) -> NewExemplar:
    value = a + b

    def render(x: Fraction) -> str:
        return f"{float(x):.1f}".rstrip("0").rstrip(".") if x == x.__round__() else f"{float(x):.1f}"

    def show(x: Fraction) -> str:
        text = f"{float(x):.1f}"
        return text
    problem = f"Compute ${_signed_dec(a)} + {_after_op_dec(b)}$."
    if (a < 0) == (b < 0):
        sketch = f"Same signs: ${show(abs(a))} + {show(abs(b))} = {show(abs(value))}$, keep the sign: ${show(value) if value>=0 else '-'+show(abs(value))}$."
    else:
        bigger, smaller = (abs(a), abs(b)) if abs(a) >= abs(b) else (abs(b), abs(a))
        sketch = f"Different signs: ${show(bigger)} - {show(smaller)} = {show(abs(value))}$, keep the sign of the larger absolute value."
    return _exact(problem, _dec_answer(value), sketch)


def _signed_dec(x: Fraction) -> str:
    return f"{float(x):.1f}"


def _after_op_dec(x: Fraction) -> str:
    v = f"{float(x):.1f}"
    return f"({v})" if x < 0 else v


def _dec_answer(value: Fraction) -> str:
    if value == value.__round__():
        return str(int(value))
    return f"{float(value):.1f}"


def _decimal_muldiv(a: Fraction, op: str, b: Fraction) -> NewExemplar:
    value = a * b if op == "*" else a / b
    symbol = r"\times" if op == "*" else r"\div"
    problem = f"Compute ${_signed_dec(a)} {symbol} {_after_op_dec(b)}$."
    same = (a < 0) == (b < 0)
    sign_word = "positive" if same else "negative"
    sketch = f"Signs {'match' if same else 'differ'}, giving a {sign_word} result: ${abs(float(a)):g} {symbol} {abs(float(b)):g} = {abs(float(value)):g}$."
    return _exact(problem, _dec_answer(value), sketch)


def _frac_str(f: Fraction) -> str:
    if f.denominator == 1:
        return str(f.numerator)
    sign = "-" if f < 0 else ""
    return f"{sign}\\frac{{{abs(f.numerator)}}}{{{f.denominator}}}"


def _frac_answer(f: Fraction) -> str:
    if f.denominator == 1:
        return str(f.numerator)
    return f"{f.numerator}/{f.denominator}"


def _frac_add(a: Fraction, op: str, b: Fraction) -> NewExemplar:
    value = a + b if op == "+" else a - b
    symbol = "+" if op == "+" else "-"
    b_text = f"\\left({_frac_str(b)}\\right)" if b < 0 else _frac_str(b)
    problem = f"Compute ${_frac_str(a)} {symbol} {b_text}$."
    lcd = a.denominator * b.denominator // _gcd(a.denominator, b.denominator)
    a_scaled = a.numerator * (lcd // a.denominator)
    b_scaled = b.numerator * (lcd // b.denominator)
    result_num = a_scaled + b_scaled if op == "+" else a_scaled - b_scaled
    sketch = f"LCD ${lcd}$: $\\frac{{{a_scaled}}}{{{lcd}}} {symbol} \\frac{{{b_scaled}}}{{{lcd}}} = \\frac{{{result_num}}}{{{lcd}}} = {_frac_str(value)}$."
    return _exact(problem, _frac_answer(value), sketch)


def _gcd(a: int, b: int) -> int:
    while b:
        a, b = b, a % b
    return a


def _frac_muldiv(a: Fraction, op: str, b: Fraction) -> NewExemplar:
    value = a * b if op == "*" else a / b
    symbol = r"\times" if op == "*" else r"\div"
    b_text = f"\\left({_frac_str(b)}\\right)" if b < 0 else _frac_str(b)
    problem = f"Compute ${_frac_str(a)} {symbol} {b_text}$."
    negatives = (a < 0) + (b < 0)
    sign_word = "negative" if negatives % 2 == 1 else "positive"
    if op == "*":
        raw_num, raw_den = abs(a.numerator) * abs(b.numerator), a.denominator * b.denominator
    else:
        raw_num, raw_den = abs(a.numerator) * b.denominator, a.denominator * abs(b.numerator)
    reduced = abs(value)
    chain = f"\\frac{{{raw_num}}}{{{raw_den}}}"
    if (raw_num, raw_den) != (reduced.numerator, reduced.denominator):
        chain += f" = {_frac_str(reduced)}"
    sketch = f"{negatives} negative factor{'s' if negatives != 1 else ''}: {sign_word}; ${chain}$."
    return _exact(problem, _frac_answer(value), sketch)


# ---------------------------------------------------------------------------
# Word-problem helpers (no LaTeX `Compute $...$.` shape to cross-evaluate
# with `foundations_compute`; the semantic test recomputes each by an
# independent Python expression of its own, not by calling these).
# ---------------------------------------------------------------------------


def _net_change(problem: str, start: int, steps: list[int]) -> NewExemplar:
    value = start
    trace = [str(start)]
    for step in steps:
        value += step
        trace.append(f"{'+' if step >= 0 else '-'} {abs(step)}")
    computed = start + sum(steps)
    assert computed == value
    sketch = "$" + " ".join(trace) + f" = {value}$."
    return _exact(problem, str(value), sketch, contract=EXACT)


def _repeated_change(problem: str, start: int, rate: int, count: int) -> NewExemplar:
    value = start + rate * count
    sign = "+" if rate >= 0 else "-"
    sketch = f"${start} {sign} {count} \\times {abs(rate)} = {value}$."
    return _exact(problem, str(value), sketch, contract=EXACT)


def _single_change(problem: str, start: int, delta: int) -> NewExemplar:
    value = start + delta
    sign = "+" if delta >= 0 else "-"
    sketch = f"${start} {sign} {abs(delta)} = {value}$."
    return _exact(problem, str(value), sketch)


def _gap(problem: str, high: int, low: int) -> NewExemplar:
    value = high - low
    sketch = f"${high} - {_after_op(low)} = {value}$."
    return _exact(problem, str(value), sketch)


# ---------------------------------------------------------------------------
# Non-arithmetic families: comparisons, chains, absolute value, opposites,
# number-line placement.
# ---------------------------------------------------------------------------


def _opposite(n: int) -> NewExemplar:
    value = -n
    problem = f"What is the opposite of ${n}$?"
    sketch = f"The opposite is the mirror image across $0$: ${n}$ and ${value}$ are the same distance from $0$."
    return _exact(problem, str(value), sketch)


def _nested_opposite(depth: int, n: int) -> NewExemplar:
    inner = str(n)
    expr = inner
    for _ in range(depth):
        expr = f"-({expr})"
    value = n if depth % 2 == 0 else -n
    problem = f"Compute ${expr}$."
    parity = "even" if depth % 2 == 0 else "odd"
    sign_word = "sign" if depth == 1 else "signs"
    sketch = f"{depth} minus {sign_word} ({parity}): the result is {'the number itself' if depth % 2 == 0 else 'its opposite'}, ${value}$."
    return _exact(problem, str(value), sketch)


def _left_of_zero(n: int) -> NewExemplar:
    problem = f"What integer is ${n}$ units to the left of $0$ on the number line?"
    sketch = f"Left of $0$ is negative; ${n}$ units left is ${-n}$."
    return _exact(problem, str(-n), sketch)


def _two_moves(first: int, second_left: int) -> NewExemplar:
    """Start at 0, move `first` units right, then `second_left` units left."""
    value = first - second_left
    problem = f"Start at $0$ and move ${first}$ units to the right, then ${second_left}$ units to the left. What integer do you land on?"
    sketch = f"$0 + {first} = {first}$; ${first} - {second_left} = {value}$."
    return _exact(problem, str(value), sketch)


def _which_greater(a: int, b: int) -> NewExemplar:
    """`Which is greater, a or b?`, matching this KP's own established shape
    (a plain-number answer), unlike `comparing-integers`' relation-symbol
    fill-in-the-blank shape.
    """
    greater = max(a, b)
    problem = f"Which is greater, ${a}$ or ${b}$?"
    sketch = f"${max(a, b)}$ is to the right of ${min(a, b)}$ on the number line."
    return _exact(problem, str(greater), sketch)


def _compare(a: int, b: int) -> NewExemplar:
    symbol = "<" if a < b else ">"
    problem = f"Fill in $<$ or $>$: ${a} \\;\\square\\; {b}$."
    sketch = f"${a}$ is {'left' if symbol == '<' else 'right'} of ${b}$ on the number line, so ${a} {symbol} {b}$."
    return _exact(problem, symbol, sketch, contract=LABEL_LT_GT)


def _truth(a: int, b: int, relation: str) -> NewExemplar:
    actual = "true" if (a < b if relation == "<" else a > b) else "false"
    problem = f"True or false: ${a} {relation} {b}$."
    truth_symbol = "<" if a < b else ">" if a > b else "="
    sketch = f"${a}$ compares to ${b}$ with ${truth_symbol}$, so the statement is {actual}."
    return _exact(problem, actual, sketch, contract=LABEL_TRUE_FALSE)


def _chain_answer(values: list[int]) -> NewExemplar:
    ordered = sorted(values)
    assert ordered == sorted(set(ordered)), "a chain contract needs strictly increasing links"
    numbers = ", ".join(f"${v}$" for v in values)
    problem = f"Write {numbers} as a chain from least to greatest using $<$."
    answer = " < ".join(str(v) for v in ordered)
    sketch = "On the number line the order least to greatest is " + " < ".join(str(v) for v in ordered) + "."
    return _exact(problem, answer, sketch, contract=ASCENDING_CHAIN)


def _distance(a: int, b: int) -> NewExemplar:
    value = abs(a - b)
    problem = f"How many units apart are ${a}$ and ${b}$ on the number line?"
    if (a < 0) != (b < 0) or a == 0 or b == 0:
        sketch = f"From ${a}$ to $0$ is ${abs(a)}$, from $0$ to ${b}$ is ${abs(b)}$; total ${value}$."
    else:
        sketch = f"Count from ${a}$ up to ${b}$: ${value}$ units."
    return _exact(problem, str(value), sketch)


def _abs_value(n: int) -> NewExemplar:
    problem = f"Compute $|{n}|$."
    return _exact(problem, str(abs(n)), f"The distance of ${n}$ from $0$ is ${abs(n)}$.")


def _abs_combo(a: int, op: str, b: int) -> NewExemplar:
    value = abs(a) + abs(b) if op == "+" else abs(a) - abs(b)
    symbol = "+" if op == "+" else "-"
    problem = f"Compute $|{a}| {symbol} |{b}|$."
    sketch = f"$|{a}| = {abs(a)}$, $|{b}| = {abs(b)}$; ${abs(a)} {symbol} {abs(b)} = {value}$."
    return _exact(problem, str(value), sketch)


def _order_least_greatest(values: list[int]) -> NewExemplar:
    ordered = sorted(values)
    numbers = ", ".join(f"${v}$" for v in values)
    problem = f"Order from least to greatest: {numbers}."
    answer = ", ".join(str(v) for v in ordered)
    sketch = "Placed on the number line, the order is " + ", ".join(str(v) for v in ordered) + "."
    return _exact(problem, answer, sketch)


def _plain(value: Fraction, tex: str) -> str:
    """The plain (non-LaTeX) answer text for `value`, matching this file's own
    convention: a value the problem displayed as a decimal answers as a
    decimal; a value displayed as a fraction answers as `n/d`.
    """
    if "." in tex:
        return _dec_answer(value)
    return _frac_answer(value)


def _order_rationals(values: list[Fraction], texts: list[str]) -> NewExemplar:
    paired = sorted(zip(values, texts), key=lambda item: item[0])
    problem = "Order from least to greatest: " + ", ".join(f"${t}$" for t in texts) + "."
    answer = ", ".join(_plain(v, t) for v, t in paired)
    decimals = ", ".join(f"{float(v):.3g}" for v, _ in paired)
    sketch = f"As decimals, the least-to-greatest order is {decimals}."
    return _exact(problem, answer, sketch)


def _compare_rationals(a: Fraction, a_text: str, b: Fraction, b_text: str) -> NewExemplar:
    greater_text = a_text if a > b else b_text
    greater_plain = _plain(a, a_text) if a > b else _plain(b, b_text)
    problem = f"Which is greater, ${a_text}$ or ${b_text}$?"
    sketch = f"${a_text} \\approx {float(a):.3g}$ and ${b_text} \\approx {float(b):.3g}$; the greater value is ${greater_text}$."
    return _exact(problem, greater_plain, sketch)


def _mixed_add(whole_a: int, num_a: int, den_a: int, op: str, whole_b: int, num_b: int, den_b: int) -> NewExemplar:
    a = Fraction(whole_a * den_a + (num_a if whole_a >= 0 else -num_a), den_a) if whole_a < 0 else Fraction(whole_a * den_a + num_a, den_a)
    b = Fraction(whole_b * den_b + (num_b if whole_b >= 0 else -num_b), den_b) if whole_b < 0 else Fraction(whole_b * den_b + num_b, den_b)
    value = a + b if op == "+" else a - b
    symbol = "+" if op == "+" else "-"

    def mixed_tex(whole: int, num: int, den: int) -> str:
        sign = "-" if whole < 0 else ""
        return f"{sign}{abs(whole)}\\frac{{{num}}}{{{den}}}"

    raw_b_text = mixed_tex(whole_b, num_b, den_b)
    b_text = f"\\left({raw_b_text}\\right)" if whole_b < 0 else raw_b_text
    problem = f"Compute ${mixed_tex(whole_a, num_a, den_a)} {symbol} {b_text}$."
    lcd = a.denominator * b.denominator // _gcd(a.denominator, b.denominator)
    sketch = f"Improper form: $\\frac{{{a.numerator * (lcd // a.denominator)}}}{{{lcd}}} {symbol} \\frac{{{b.numerator * (lcd // b.denominator)}}}{{{lcd}}} = {_frac_str(value)}$."
    return _exact(problem, _frac_answer(value), sketch)
