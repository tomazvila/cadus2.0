"""Twelve exhaustive, pending-only word-model and graph recipe replacements."""
import itertools
import json
from pathlib import Path

EXACT = {"kind": "exact"}
PAIR = {"kind": "coordinates", "arity": 2}
YESNO = {"kind": "label", "options": [["yes"], ["no"]]}
ROOT = Path(__file__).resolve().parents[3]
DEST = ROOT / "docs/content-foundations/template36/graphs.json"


def multipart(**parts):
    return {"kind": "multipart", "parts": [
        {"name": name, "contract": contract} for name, contract in parts.items()]}


def flags(**values):
    return "; ".join(f"{name} = {'yes' if value else 'no'}" for name, value in values.items())


def recipe(key, statement, expr, domains, solve, sketch, hints, contract=EXACT):
    samples = []
    for values in itertools.product(*domains.values()):
        params = dict(zip(domains, values))
        samples.append({"params": params, "expected": solve(**params)})
    return {"kp_id": key, "kind": "template", "arguments": {
        "statement": statement, "answer_expr": expr, "answer_contract": contract,
        "params": {name: {"kind": "choice", "values": values}
                   for name, values in domains.items()}, "constraints": [],
        "samples": samples, "solution_sketch": sketch, "hints": hints,
        "distractors": []}}


def word_rows():
    return [recipe("consecutive-integer-problems/kp1",
        "Subtract ${a}$ from five times an unknown integer. The result is ${b}$. Find the integer.",
        "(b+a)/5", {"a": [11, 16, 21], "b": [69, 89, 109, 129]},
        lambda a, b: str((b+a)//5),
        "Let the unknown be x. The sentence gives 5x-{a}={b}. Add {a} to both sides, "
        "then divide by five: x=({b}+{a})/5. Substituting this expression into "
        "5x-{a} cancels the added amount and recovers {b}.",
        ["Translate the multiplication before the subtraction.",
         "Undo the subtraction, then undo multiplication by five."]),
        recipe("equation-word-problems/kp1",
        "A workshop divides ${t}$ brass pieces between two bins. The larger bin gets ${d}$ "
        "more pieces than the smaller bin. How many pieces go in the larger bin?",
        "(t+d)/2", {"t": [82, 94, 106], "d": [12, 18, 24, 30]},
        lambda t, d: str((t+d)//2),
        "If the smaller bin gets x pieces, the larger gets x+{d}. The total equation "
        "is 2x+{d}={t}. Thus x=({t}-{d})/2, and the requested larger amount is "
        "({t}+{d})/2. Their difference is {d} and their sum is {t}.",
        ["Name the smaller amount and express the larger using it.",
         "The question asks for the larger amount; check both the sum and the difference."]),
        recipe("equation-word-problems/kp3",
        "An aunt is three times her niece's current age. In ${t}$ years, the aunt will be "
        "twice her niece's age then. How old is the aunt now, in years?",
        "3*t", {"t": list(range(13, 25))}, lambda t: str(3*t),
        "Let n be the niece's current age, so the aunt's is 3n. Advancing both ages gives "
        "3n+{t}=2(n+{t}). Expanding and subtracting 2n+{t} gives n={t}. The requested "
        "current aunt's age is 3({t}); adding {t} to each current age verifies the future ratio.",
        ["Advance both people's ages by the same number of years.",
         "Solve for the niece first, then use the present-day relationship for the aunt."])]


def qualitative_rows():
    return [recipe("interpreting-graphs-qualitatively/kp1",
        "A graph has time in hours on its horizontal axis and water depth in centimetres on its "
        "vertical axis. Its straight segment joins $(2,{a})$ to $(6,{b})$. Describe the depth "
        "by entering increasing and decreasing as yes/no fields. Both no means constant.",
        "multipart(equalitylabel(signcase(b-a,[0,0,1]),1),"
        "equalitylabel(signcase(b-a,[1,0,0]),1))",
        {"a": [11, 17, 23, 29], "b": [17, 23, 29]},
        lambda a, b: flags(increasing=b>a, decreasing=b<a),
        "Time runs from 2 to 6, so read the endpoints left to right. Compare ending height "
        "{b} with starting height {a}. A higher ending height means increasing, a lower "
        "one means decreasing, and equal heights make the entire straight segment constant.",
        ["Read the graph from earlier time to later time.",
         "Compare the two vertical coordinates; horizontal movement alone says nothing about depth."],
        multipart(increasing=YESNO, decreasing=YESNO)),
        recipe("interpreting-graphs-qualitatively/kp2",
        "A graph's horizontal axis measures elapsed hours and its vertical axis measures total "
        "distance travelled in kilometres. The marked point is $({t},{d})$. Interpret that point "
        "by completing elapsed = ...; distance = ... (hours, then kilometres).",
        "multipart(t,d)", {"t": [7, 11, 13], "d": [37, 43, 53, 61]},
        lambda t, d: f"elapsed = {t}; distance = {d}",
        "The first coordinate belongs to the horizontal elapsed-time axis and the second to the "
        "vertical total-distance axis. Thus elapsed time is {t} hours and the travelled distance "
        "at that time is {d} kilometres. The point gives a total, not a per-hour rate.",
        ["Match each coordinate to its named axis before attaching the units.",
         "The vertical axis records total distance at that time."],
        multipart(elapsed=EXACT, distance=EXACT)),
        recipe("interpreting-graphs-qualitatively/kp3",
        "Two straight distance-time segments use the same axes: time in minutes horizontally and "
        "distance in metres vertically. Both start at $(0,0)$. At minute 4, runner A's segment ends "
        "at $(4,{a})$ and runner B's at $(4,{b})$. Without calculating slopes, compare steepness. "
        "Enter faster_a and equal_speed as yes/no fields; both no means B is faster.",
        "multipart(equalitylabel(signcase(a-b,[0,0,1]),1),equalitylabel(a,b))",
        {"a": [13, 17, 23, 29], "b": [13, 19, 31]},
        lambda a, b: flags(faster_a=a>b, equal_speed=a==b),
        "Both segments span the same four minutes from the same starting distance. Compare the "
        "heights {a} and {b}. The taller segment is steeper and represents more distance in the "
        "same time, hence greater speed. Equal heights produce coincident segments and equal speeds.",
        ["The two segments have equal horizontal widths.",
         "A higher endpoint above the same start means a steeper segment."],
        multipart(faster_a=YESNO, equal_speed=YESNO))]


def model_rows():
    rates = [-19, -17, -13, -11, -7, -3, 2, 6, 10, 14, 18, 22]
    return [recipe("interpreting-linear-models/kp1",
        "A reservoir's volume follows $V=({r})t+800$, where V is litres and t is minutes, "
        "for $0<=t<=10$. Interpret the slope: enter rate as the signed change in litres per "
        "minute and filling as yes/no for whether the reservoir gains water.",
        "multipart(r,equalitylabel(signcase(r,[0,0,1]),1))", {"r": rates},
        lambda r: f"rate = {r}; filling = {'yes' if r>0 else 'no'}",
        "Increasing t by one changes volume from ({r})t+800 to ({r})(t+1)+800, a difference "
        "of {r} litres. Thus the signed rate is {r} litres per minute. Its sign indicates "
        "gain or loss; the initial 800 litres contributes nothing to that one-minute change.",
        ["Ask how the predicted volume changes when time increases by one minute.",
         "Use the sign of the coefficient to distinguish filling from draining."],
        multipart(rate=EXACT, filling=YESNO)),
        recipe("interpreting-linear-models/kp2",
        "A repair bill is modelled by $C=17h+{b}$, with C in euros and h in hours of work. "
        "Interpret the vertical intercept: what fixed fee in euros is charged even when no "
        "hours of work are used? Enter the amount.",
        "b", {"b": list(range(61, 85, 2))}, lambda b: str(b),
        "The vertical intercept occurs at h=0. Substitution gives C=17(0)+{b}={b} euros. "
        "This amount remains when the hourly contribution is zero, so it is the fixed fee "
        "charged independently of the duration of the repair.",
        ["The vertical axis crossing has horizontal coordinate zero.",
         "Set hours to zero and identify the remaining charge."]),
        recipe("linear-word-problems/kp1",
        "A tool-hire company charges a fixed booking fee of EUR ${f}$ and EUR ${r}$ for each "
        "hour. Build the cost model $C=mh+b$ for h hours by entering its coefficients $(m,b)$.",
        "(r,f)", {"f": [29, 37, 43], "r": [7, 11, 13, 17]},
        lambda f, r: f"({r},{f})",
        "For h hours, the variable charge is {r}h euros. Add the fixed {f} euro booking "
        "fee once to obtain C={r}h+{f}. Comparing with C=mh+b identifies m={r} euros per "
        "hour and b={f} euros; zero hours gives the booking fee.",
        ["Multiply the per-hour rate by the number of hours.",
         "Add the booking fee once, then read the coefficient order requested."], PAIR)]


def line_rows():
    return [recipe("parallel-perpendicular-lines/kp1",
        "Write the line parallel to $y=({m})x+41$ through $(3,{y})$. Enter the "
        "coefficients $(m,b)$ of its slope-intercept equation $y=mx+b$.",
        "(m,y-3*m)", {"m": [-7, -4, 6], "y": [19, 23, 31, 37]},
        lambda m, y: f"({m},{y-3*m})",
        "Parallel lines have equal slopes, so retain {m}. Substitute the required point "
        "into y=({m})x+b: {y}=3({m})+b, giving b={y}-3({m}). This restores the given "
        "height at x=3; the different intercept makes the required line distinct.",
        ["A parallel line keeps the original rate of change.",
         "Insert both point coordinates to determine the new intercept."], PAIR),
        recipe("plotting-points/kp3",
        "A point is on the x-axis at signed horizontal position ${a}$. Give its ordered "
        "pair $(x,y)$, writing the horizontal coordinate first.",
        "(a,0)", {"a": [-31, -27, -23, -19, -15, -11, 11, 15, 19, 23, 27, 31]},
        lambda a: f"({a},0)",
        "A point on the horizontal x-axis has no vertical displacement, hence y=0. "
        "The signed horizontal position is {a}, so x={a}. Put the horizontal position "
        "first and the zero height second; the sign preserves the side of the origin.",
        ["Which coordinate measures displacement away from the horizontal axis?",
         "Every point on the x-axis has the same vertical coordinate."], PAIR),
        recipe("slopes-of-parallel-perpendicular-lines/kp1",
        "A reference line has equation $y=({m})x+47$. A distinct parallel line crosses "
        "the y-axis at 53. What is the slope of that parallel line?",
        "m", {"m": [-23, -19, -17, -13, -11, -7, 6, 8, 10, 12, 14, 16]},
        lambda m: str(m),
        "The coefficient of x in the reference equation is {m}. Distinct parallel lines "
        "have equal slopes and different vertical intercepts. Therefore preserve that "
        "coefficient in the new equation y=({m})x+53; its slope is {m}.",
        ["Identify the coefficient multiplying x in the reference equation.",
         "Moving a line vertically changes its intercept while preserving its slope."])]


def build():
    return word_rows() + qualitative_rows() + model_rows() + line_rows()


if __name__ == "__main__":
    DEST.parent.mkdir(parents=True, exist_ok=True)
    DEST.write_text(json.dumps(build(), indent=2) + "\n")
