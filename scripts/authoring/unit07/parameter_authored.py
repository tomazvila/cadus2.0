"""Explicit response conventions that retain every coefficient of the result."""
from residual_structured import EXACT, POINT, multipart

ROOT_COEFFICIENTS = {
    "writing-quadratics-from-roots/kp1": [(1, 2, -15), (1, -6, 8), (1, -3, -10), (1, 11, 28)],
    "writing-quadratics-from-roots/kp2": [(1, -8, 16), (1, 6, 0), (1, 6, 9), (1, -5, 0)],
    "writing-quadratics-from-roots/kp3": [(2, 5, -3), (3, -5, 2), (4, 7, -2), (5, -8, 3)],
    "applying-the-quadratic-formula/kp1": [(3, -5, 2), (1, -4, 3), (4, 3, -1), (1, -3, 5)],
}
VERTEX_PARAMETERS = {
    "converting-to-vertex-form/kp1": [(1, -3, -4), (1, 2, 3), (1, -4, -6), (1, 5, -4)],
    "converting-to-vertex-form/kp2": [(2, 2, -5), (-1, 1, 4), (3, 2, -7), (-2, -2, 9)],
    "converting-to-vertex-form/kp3": [(1, -5, 5), (3, -1, -2), (1, -6, 4), (2, -1, 3)],
}
ROOT_FACTORS = {
    "writing-quadratics-from-roots/kp1": ["(x-3)(x+5)", "(x-2)(x-4)", "(x-5)(x+2)", "(x+4)(x+7)"],
    "writing-quadratics-from-roots/kp2": ["(x-4)^2", "x(x+6)", "(x+3)^2", "x(x-5)"],
    "writing-quadratics-from-roots/kp3": ["(2x-1)(x+3)", "(3x-2)(x-1)", "(4x-1)(x+2)", "(5x-3)(x-1)"],
}


def append_once(text, suffix):
    return text if text.endswith(suffix) else text + suffix


def coefficients(key, index, existing):
    values = ROOT_COEFFICIENTS[key][index]
    a, b, c = values
    suffix = " Represent the equation as $Ax^2+Bx+C=0$ and return its coefficient tuple (A,B,C)."
    problem = append_once(existing["problem"], suffix)
    if key in ROOT_FACTORS:
        product = ROOT_FACTORS[key][index]
        sketch = (f"The roots give ${product}=0$. Expanding gives $({a})x^2+({b})x+({c})=0$. "
                  "The coefficient tuple lists all three coefficients, including a zero if a power is absent.")
        if key.endswith("kp3"):
            sketch += " Clearing each root denominator gives integer coefficients with no common factor."
    else:
        sketch = (f"Move every term to the left and combine like powers: $({a})x^2+({b})x+({c})=0$. "
                  "Read the signed coefficients of the quadratic term, linear term, and constant in that order.")
    return dict(problem=problem, answer=str(values), answer_contract=EXACT, solution_sketch=sketch)


def vertex(key, index, existing):
    a, h, k = VERTEX_PARAMETERS[key][index]
    monic = key.endswith("kp1")
    interpret = key.endswith("kp3")
    suffix = (" Represent the full vertex form as $y=(x-h)^2+k$ by returning (h,k)." if monic else
              " Represent the full vertex form as $y=A(x-h)^2+k$ by returning (A,h,k).")
    contract = EXACT
    answer = str((h, k) if monic else (a, h, k))
    if interpret:
        field = "minimum" if index in (0, 2) else "vertex"
        suffix = f" Use $y=A(x-h)^2+k$ and report vertex_parameters = (A,h,k) and {field}."
        answer = f"vertex_parameters = {(a, h, k)}; {field} = {k if field == 'minimum' else (h, k)}"
        contract = multipart(vertex_parameters=EXACT, **{field: EXACT if field == "minimum" else POINT})
    sketch = (f"Complete the inner square after extracting $({a})$ from the variable terms. "
              f"The square-completion adjustment inside is $({h})^2$, multiplied outside by $({a})$. "
              f"The result is $y=({a})(x-({h}))^2+({k})$.")
    if interpret:
        sketch += f" The vertex is $({h},{k})$; the positive leading coefficient makes ${k}$ the minimum."
    return dict(problem=append_once(existing["problem"], suffix), answer=answer,
                answer_contract=contract, solution_sketch=sketch)


def completion(index, existing):
    h = [4, -3, 5, -2][index]
    suffix = " Express the completed square as $(x+h)^2$; return constant = the added constant and shift = h."
    sketch = (f"Half the signed linear coefficient is ${h}$. Its square is ${h*h}$, the required added constant. "
              f"Expansion checks $(x+({h}))^2=x^2+({2*h})x+{h*h}$.")
    return dict(problem=append_once(existing["problem"], suffix),
                answer=f"constant = {h*h}; shift = {h}",
                answer_contract=multipart(constant=EXACT, shift=EXACT), solution_sketch=sketch)


def updates(key, index, existing):
    if key in ROOT_COEFFICIENTS:
        return coefficients(key, index, existing)
    if key in VERTEX_PARAMETERS:
        return vertex(key, index, existing)
    if key == "completing-the-square/kp1":
        return completion(index, existing)
    return {}
