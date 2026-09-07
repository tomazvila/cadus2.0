"""Conservative cross-key premise screening for the graph/model recipe lane.

The independent oracle handles candidate mathematics. This screen ignores names,
punctuation, numeric order and repeated mentions: equal mathematical operands in
the same task family require review even when the prose and KP keys differ.
"""
from fractions import Fraction
import re

FAMILIES = {
    "consecutive-integer-problems/kp1": "number-equation",
    "equation-word-problems/kp1": "shared-total",
    "equation-word-problems/kp3": "age-comparison",
    "interpreting-graphs-qualitatively/kp1": "graph-trend",
    "interpreting-graphs-qualitatively/kp2": "graph-point",
    "interpreting-graphs-qualitatively/kp3": "graph-steepness",
    "interpreting-linear-models/kp1": "model-rate",
    "interpreting-linear-models/kp2": "model-intercept",
    "linear-word-problems/kp1": "cost-model",
    "parallel-perpendicular-lines/kp1": "parallel-through",
    "plotting-points/kp3": "axis-point",
    "slopes-of-parallel-perpendicular-lines/kp1": "parallel-slope",
}
WORDS = dict(zip("zero one two three four five six seven eight nine ten eleven twelve".split(), range(13)))
WORDS.update(twice=2, double=2, triple=3)


def operands(problem):
    text = problem.lower()
    for word, number in WORDS.items():
        text = re.sub(r"\b"+word+r"\b", str(number), text)
    return frozenset(Fraction(x) for x in re.findall(r"(?<![\w.])-?\d+(?:\.\d+)?", text))


def families(key, problem):
    result = {FAMILIES[key]} if key in FAMILIES else set()
    p = problem.lower()
    rules = {
        "number-equation": r"unknown integer|times (a|the|an unknown) number|find the number",
        "shared-total": r"(larger|smaller|shorter|longer).*(total|pieces|bin)|two.*(sum|combined|total)",
        "age-comparison": r"\bages?\b|years (old|older)|as old as",
        "graph-trend": r"(graph|segment).*(increasing|decreasing|constant|rising|falling)",
        "graph-point": r"graph.*(point|shown)|marked point|point.*graph",
        "graph-steepness": r"steeper|steepness|faster.*(runner|growth|hiker)|runner.*faster",
        "model-rate": r"interpret.*slope|mean.*per (hour|minute)|signed change|slope.*(context|rate)",
        "model-intercept": r"fixed fee|starting (amount|height)|interpret.*intercept",
        "cost-model": r"(fee|charges|cost).*(write|build|expression|model|equation)|(write|build|expression|model).*(fee|charges|cost)",
        "parallel-through": r"parallel.*through|through.*parallel",
        "axis-point": r"(coordinates|ordered pair).*(axis|origin)|(axis|origin).*(coordinates|ordered pair)",
        "parallel-slope": r"slope.*parallel|parallel.*slope",
    }
    for family, pattern in rules.items():
        if re.search(pattern, p):
            result.add(family)
    return result


def fingerprints(key, problem):
    values = operands(problem)
    return {(family, values) for family in families(key, problem)}


def collisions(candidates, corpus):
    index = {}
    for row in corpus:
        for fingerprint in fingerprints(row["kp_id"], row["problem"]):
            index.setdefault(fingerprint, []).append(row)
    hits = []
    for key, problem in candidates:
        for fingerprint in fingerprints(key, problem):
            for other in index.get(fingerprint, []):
                hits.append((key, problem, other))
    return hits
