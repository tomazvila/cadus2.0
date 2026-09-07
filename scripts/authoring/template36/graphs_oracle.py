"""Independent statement reconstruction; never evaluates an authored answer_expr."""
from fractions import Fraction
import re


def fields(**values):
    def render(value):
        return ("yes" if value else "no") if isinstance(value, bool) else str(value)
    return "; ".join(f"{name} = {render(value)}" for name, value in values.items())


def pair(x, y):
    return f"({x},{y})"


def captures(pattern, statement):
    matched = re.search(pattern, statement)
    if not matched:
        raise ValueError(f"Statement does not match independently reviewed grammar: {statement}")
    return tuple(int(x) for x in matched.groups())


def word_answer(key, statement):
    if key == "consecutive-integer-problems/kp1":
        subtract, result = captures(r"Subtract \$(-?\d+)\$.*result is \$(-?\d+)\$", statement)
        solutions = [x for x in range(-1000, 1001) if sum([x]*5)-subtract == result]
        assert len(solutions) == 1
        return str(solutions[0]), ("number-affine", 5, -subtract, result)
    if key == "equation-word-problems/kp1":
        total, difference = captures(r"divides \$(\d+)\$.*gets \$(\d+)\$", statement)
        allocations = [(x, total-x) for x in range(total+1) if total-2*x == difference]
        assert len(allocations) == 1
        return str(allocations[0][1]), ("two-parts-larger", total, difference)
    years, = captures(r"In \$(\d+)\$ years", statement)
    ages = [(n, 3*n) for n in range(1, 121) if 3*n+years == 2*(n+years)]
    assert len(ages) == 1
    return str(ages[0][1]), ("age-shift-older", 3, 2, years)


def qualitative_answer(key, statement):
    kp = key.split("/")[1]
    if kp == "kp1":
        start, end = captures(r"joins \$\(2,(\d+)\)\$ to \$\(6,(\d+)\)\$", statement)
        direction = sorted([(start, "earlier"), (end, "later")])
        rising = start != end and direction[0][1] == "earlier"
        falling = start != end and direction[0][1] == "later"
        return fields(increasing=rising, decreasing=falling), ("depth-trend", 2, start, 6, end)
    if kp == "kp2":
        hours, distance = captures(r"marked point is \$\((\d+),(\d+)\)\$", statement)
        assert "horizontal axis measures elapsed hours" in statement
        assert "vertical axis measures total distance travelled in kilometres" in statement
        return fields(elapsed=hours, distance=distance), ("distance-point", hours, distance)
    first, second = captures(r"A's segment ends at \$\(4,(\d+)\)\$.*B's at \$\(4,(\d+)\)\$", statement)
    assert "Both start at $(0,0)$" in statement
    return fields(faster_a=max(first, second) == first and first != second,
                  equal_speed=first == second), ("runner-steepness", 4, first, second)


def model_answer(key, statement):
    if key == "interpreting-linear-models/kp1":
        coefficient, = captures(r"V=\((-?\d+)\)t\+800", statement)
        volume_at_two = 800 + coefficient + coefficient
        volume_at_one = 800 + coefficient
        change = volume_at_two-volume_at_one
        assert min(800, 800+10*coefficient) >= 0
        return fields(rate=change, filling=volume_at_two>volume_at_one), ("volume-rate", coefficient, 800)
    if key == "interpreting-linear-models/kp2":
        fee, = captures(r"C=17h\+(\d+)", statement)
        return str(17*0+fee), ("repair-intercept", 17, fee)
    fee, rate = captures(r"fee of EUR \$(\d+)\$ and EUR \$(\d+)\$", statement)
    bills = [fee+sum([rate]*hours) for hours in range(3)]
    return pair(bills[1]-bills[0], bills[0]), ("cost-model", rate, fee)


def line_answer(key, statement):
    if key == "plotting-points/kp3":
        x, = captures(r"horizontal position \$(-?\d+)\$", statement)
        assert "on the x-axis" in statement
        return pair(x, 0), ("axis-point", x, 0)
    if key == "slopes-of-parallel-perpendicular-lines/kp1":
        m, = captures(r"y=\((-?\d+)\)x\+47", statement)
        p, q = (0, 47), (1, m+47)
        rise_over_run = Fraction(q[1]-p[1], q[0]-p[0])
        return str(rise_over_run), ("parallel-slope", m, 47, 53)
    m, y = captures(r"y=\((-?\d+)\)x\+41\$ through \$\(3,(-?\d+)\)\$", statement)
    intercepts = [b for b in range(-1000, 1001) if sum([m]*3)+b == y]
    assert len(intercepts) == 1 and intercepts[0] != 41
    return pair(m, intercepts[0]), ("parallel-through", m, 41, 3, y)


def reconstruct_with_signature(key, statement):
    topic = key.split("/")[0]
    if topic in ("consecutive-integer-problems", "equation-word-problems"):
        return word_answer(key, statement)
    if topic == "interpreting-graphs-qualitatively":
        return qualitative_answer(key, statement)
    if topic in ("interpreting-linear-models", "linear-word-problems"):
        return model_answer(key, statement)
    if topic in ("parallel-perpendicular-lines", "plotting-points", "slopes-of-parallel-perpendicular-lines"):
        return line_answer(key, statement)
    raise ValueError(f"Unsupported key: {key}")


def reconstruct(key, statement):
    return reconstruct_with_signature(key, statement)[0]


def signature(key, statement):
    return reconstruct_with_signature(key, statement)[1]


def verify(key, statement, expected):
    actual = reconstruct(key, statement)
    assert actual.replace(" ", "") == expected.replace(" ", ""), (key, statement, actual, expected)
    return signature(key, statement)
