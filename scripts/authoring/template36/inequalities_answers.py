"""Reconstruct answers solely from displayed premises, without answer_expr access."""
import re
from fractions import Fraction


def numbers(text):
    return [int(value) for value in re.findall(r"[-+]?\d+", text)]


def ray(bound, comparison):
    bracket = "[" if "=" in comparison else "("
    if comparison.startswith(">"):
        return f"{bracket}{bound}, ∞)"
    bracket = "]" if "=" in comparison else ")"
    return f"(-∞, {bound}{bracket}"


def graph_answer(key, topic, text, values):
    """Reconstruct number-line and coordinate-plane graph answers."""
    if key.endswith("/kp1"):
        comparison = re.search(r"[xy] (<=|>=|<|>)", text)[1]
        styles = (
            ("closed", "open")
            if topic == "graphing-inequalities-number-line"
            else ("solid", "dashed")
        )
        return styles[0] if "=" in comparison else styles[1]
    if topic == "graphing-linear-inequalities":
        horizontal, vertical, slope, offset = values
        return "yes" if vertical <= slope * horizontal + offset else "no"
    return f"x <= {values[0]}"


def system_answer(values):
    """Solve the two displayed slope-intercept equations."""
    first_slope, first_intercept, second_slope, second_intercept = values
    xcoord = Fraction(second_intercept - first_intercept, first_slope - second_slope)
    ycoord = first_slope * xcoord + first_intercept
    assert ycoord == second_slope * xcoord + second_intercept
    return f"({xcoord}, {ycoord})"


def reconstruct(key, text):
    topic = key.split("/")[0]
    values = numbers(text)
    if topic in ("graphing-inequalities-number-line", "graphing-linear-inequalities"):
        return graph_answer(key, topic, text, values)
    if topic == "solutions-of-inequalities":
        return "true" if values[0] < values[1] else "false"
    if topic == "checking-a-solution":
        candidate, coefficient, offset, result = values
        return "yes" if coefficient * candidate + offset == result else "no"
    if topic == "writing-inequalities-from-statements":
        return f"x >= {values[0]}" if key.endswith("kp1") else f"{values[0]} h"
    if topic == "compound-inequalities":
        lower, coefficient, offset, upper = values
        low = Fraction(lower - offset, coefficient)
        high = Fraction(upper - offset, coefficient)
        return f"[{low}, {high})"
    if topic == "interval-notation":
        return reconstruct_intervals(key, text, values)
    if topic == "substitution-with-isolated-variable":
        return system_answer(values)
    raise ValueError(f"unrecognized template key: {key}")


def reconstruct_intervals(key, text, values):
    source = re.search(r"\$(.*?)\$", text)[1]
    if source.startswith("x"):
        pieces = re.findall(r"x\s*(<=|>=|<|>)\s*(-?\d+)", source)
        return " ∪ ".join(ray(int(bound), op) for op, bound in pieces)
    pieces = source.split(" ∪ ")
    results = []
    for interval in pieces:
        bound = int(re.search(r"-?\d+", interval)[0])
        if "-∞" in interval:
            op = "<=" if interval.endswith("]") else "<"
        else:
            op = ">=" if interval.startswith("[") else ">"
        results.append(f"x {op} {bound}")
    return " or ".join(results)


def semantic_signature(key, problem):
    """Separate mathematical tasks by their premises, ignoring prose and markup."""
    text = problem.replace("\\le", "<=").replace("\\ge", ">=")
    text = text.replace("≥", ">=").replace("≤", "<=").replace("−", "-")
    text = re.sub(r"\s+", " ", text)
    values = tuple(numbers(text))
    topic = key.split("/")[0]
    if topic == "solutions-of-inequalities":
        match = re.search(r"(-?\d+)\s*(<=|>=|<|>)\s*(-?\d+)", text)
        if match:
            left, op, right = int(match[1]), match[2], int(match[3])
            return (topic, left, op, right)
    if topic in ("graphing-inequalities-number-line", "graphing-linear-inequalities"):
        comparisons = tuple(re.findall(r"<=|>=|(?<![<>=])<(?![=])|(?<![<>=])>(?![=])", text))
        descriptors = tuple(word for word in ("closed", "open", "left", "right")
                            if re.search(rf"\b{word}\b", text))
        return (key, values, comparisons, descriptors)
    return (key, values, tuple(re.findall(r"<=|>=|<|>", text)))
