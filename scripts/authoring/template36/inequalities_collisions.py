"""Cross-KP task signatures: normalize premises while retaining the requested operation."""
import re


def normalize(text):
    replacements = {r"\leq": "<=", r"\geq": ">=", r"\le": "<=", r"\ge": ">=",
                    r"\lt": "<", r"\gt": ">", "≤": "<=", "≥": ">=", "−": "-",
                    r"\infty": "∞", "infinity": "∞", r"\cup": "∪", " union ": " ∪ "}
    for old, new in replacements.items():
        text = text.replace(old, new)
    text = re.sub(r"([-+])\s+(?=\d)", r"\1", text)
    return text


def math_parts(text):
    return [re.sub(r"\s+", "", part).replace("*", "") for part in re.findall(r"\$(.*?)\$", text)]


def numerals(text):
    return tuple(int(n) for n in re.findall(r"[-+]?\d+", text))


def relations(text):
    return tuple(re.findall(r"<=|>=|<|>", text))


def simple_ray(text):
    match = re.search(r"([a-z])\s*(<=|>=|<|>)\s*(-?\d+)", text)
    return (match[1], match[2], int(match[3])) if match else None


def signature(problem):
    """Return a key-independent task signature, or a precise nonmatching-family reason."""
    text = normalize(problem)
    lower = text.lower()
    math = math_parts(text)
    if "circle" in lower and ("open" in lower or "closed" in lower):
        ray = simple_ray(text)
        if ray:
            return ("endpoint-style", ray[1:]), None
        endpoint = re.search(r"(?:circle|point) at\s*\$?(-?\d+)", text)
        direction = re.search(r"\b(left|right)\b", lower)
        if endpoint and direction:
            return ("read-ray", int(endpoint[1]), "closed" in lower, direction[1]), None
    if "boundary" in lower and ("solid" in lower or "dashed" in lower):
        inequality = next((m for m in math if re.match(r"y[<>=]", m)), None)
        if inequality:
            return ("boundary-style", inequality), None
    if "true or false" in lower or "true/false" in lower:
        match = re.search(r"(-?\d+)\s*(<=|>=|<|>)\s*(-?\d+)", text)
        if match:
            return ("integer-truth", int(match[1]), match[2], int(match[3])), None
    result = substitution_signature(text, lower, math)
    if result:
        return result, None
    result = translation_signature(text, lower)
    if result:
        return result, None
    if ("solve" in lower or "system" in lower) and sum(m.count("y=") for m in math) >= 2:
        equations = tuple(sorted("y=" + part.replace("+-", "-")
                                 for part in re.findall(r"y=([^,;]+)", ",".join(math))))
        return ("equal-y-system", equations), None
    result = interval_signature(text, lower, math)
    if result:
        return result, None
    return None, "different or unsupported displayed mathematical task family"


def substitution_signature(text, lower, math):
    point = re.search(r"\(\s*(-?\d+)\s*,\s*(-?\d+)\s*\)", text)
    inequality = next((m for m in math if re.match(r"y[<>]", m)), None)
    if point and inequality and ("solution" in lower or "shaded" in lower):
        return ("point-membership", int(point[1]), int(point[2]), inequality)
    candidate = re.search(r"\b([a-z])\s*=\s*(-?\d+)\b", text)
    if candidate and "solution" in lower and "or" not in "".join(math):
        variable = candidate[1]
        equations = [m for m in math if "=" in m and m != f"{variable}={candidate[2]}"
                     and not relations(m)]
        if len(equations) == 1:
            equation = equations[0].replace(variable, "x")
            return ("candidate-equation", int(candidate[2]), equation)
    return None


def translation_signature(text, lower):
    values = numerals(text)
    if "fewest" in lower and values:
        unit = "hours" if "hours" in lower else "points" if "points" in lower else "unspecified"
        return ("context-minimum", unit, values[0])
    if "inequality" not in lower:
        return None
    comparison = next((op for phrase, op in [("at least", ">="), ("at most", "<="),
                       ("more than", ">"), ("fewer than", "<")]
                       if phrase in lower), None)
    if comparison and len(values) == 1:
        return ("translate-threshold", comparison, values[0])
    return None


def interval_signature(text, lower, math):
    if not math:
        return None
    if "solve" in lower and len(relations(math[0])) == 2 and "or" not in math[0]:
        return ("solve-compound", math[0])
    if any(word in lower for word in ("convert", "write", "solve")) and any(
            word in lower for word in ("interval", "notation", "inequalit")):
        premise = math[0]
        if "∞" in premise or re.search(r"[<>]", premise):
            direction = "from-interval" if "∞" in premise else "from-inequality"
            return ("notation-conversion", direction, premise)
    return None
