"""Targeted unit06 repairs; retain all unrelated dirty curriculum lines."""
import json
import re
from pathlib import Path

UNIT = Path("curriculum/foundations/06-exponents-radicals.yaml")

# Each replacement changes the reasoning or representation, not just operands.
# Preserve the already-correct first family in each affected knowledge point.
REPLACEMENTS = {
    ("perfect-square-roots/kp2", 1): (
        "A square garden has area $121$ square metres. Find its side length in metres.",
        "11", "$s^2=121$ and a length is positive. Since $11\\cdot11=121$, $s=11$."),
    ("perfect-square-roots/kp2", 2): (
        "The nonnegative number $s$ satisfies $s^2=100$. Find $s$.",
        "10", "Both $10$ and $-10$ square to $100$; the condition $s\\ge0$ selects $10$."),
    ("perfect-square-roots/kp2", 3): (
        "Nine equal rows contain $81$ tiles in a square array. Use the row count to find $\\sqrt{81}$.",
        "9", "$81=9\\cdot9$. The square array has $9$ tiles on each side, so its principal root is $9$."),
    ("perfect-square-roots/kp3", 1): (
        "A square panel covers $225$ square centimetres. Find its side length in centimetres.",
        "15", "$s^2=225$; $15\\cdot15=225$ and $s>0$, so the side is $15$."),
    ("perfect-square-roots/kp3", 2): (
        "Use $(20-1)^2$ to find $\\sqrt{361}$.",
        "19", "$(20-1)^2=400-40+1=361$. Hence the nonnegative root is $20-1=19$."),
    ("perfect-square-roots/kp3", 3): (
        "Factor $324=4\\cdot81$ and find its principal square root using the product rule.",
        "18", "$\\sqrt{324}=\\sqrt4\\sqrt{81}=2\\cdot9=18$; both factors are nonnegative."),
    ("square-roots/kp1", 2): (
        "Compute $\\sqrt{64}-\\sqrt{9}$.", "5",
        "$8^2=64$ and $3^2=9$. Subtract the principal roots: $8-3=5$."),
    ("square-roots/kp1", 3): (
        "A square has area $9$ square metres. Find its perimeter in metres.", "12",
        "The side is $\\sqrt9=3$ metres; four equal sides give $4\\cdot3=12$ metres."),
    ("square-roots/kp3", 2): (
        "A rectangle has side lengths $\\sqrt9$ and $\\sqrt{16}$. Find its area.", "12",
        "Area is the product of the sides: $\\sqrt9\\sqrt{16}=3\\cdot4=12$."),
    ("square-roots/kp3", 3): (
        "Find the positive factor $q$ in $\\sqrt4\\,q=\\sqrt{100}$.", "5",
        "$2q=10$, so $q=5$. Check the product: $\\sqrt4\\sqrt{25}=\\sqrt{100}$."),
    ("cube-roots/kp2", 1): (
        "Find the real number $t$ such that $t^3=-125$.", "-5",
        "$(-5)(-5)(-5)=-125$. Cubing is one-to-one on the reals, so $t=-5$."),
    ("cube-roots/kp2", 2): (
        "Use $-64=(-8)\\cdot8$ to evaluate $\\sqrt[3]{-64}$.", "-4",
        "Real cube roots preserve products: $\\sqrt[3]{-8}\\sqrt[3]8=(-2)(2)=-4$."),
    ("cube-roots/kp2", 3): (
        "A student says the real cube root of $-27$ is $3$. Give the corrected root.", "-3",
        "$3^3=27$ has the wrong sign; $(-3)^3=-27$, so the corrected root is $-3$."),
    ("pythagorean-converse/kp1", 1): (
        "Squares built on the three sides of a triangle have areas $64$, $225$, and $289$. Is the triangle right?",
        "yes", "The largest side has square area $289$. Since $64+225=289$, the converse proves a right angle."),
    ("pythagorean-converse/kp1", 2): (
        "A builder measures two sides of a triangular brace as $7$ and $24$ cm and the opposite side as $25$ cm. Are the first two sides perpendicular?",
        "yes", "$7^2+24^2=49+576=625=25^2$. The angle opposite the longest side is right."),
    ("pythagorean-converse/kp1", 3): (
        "The triple $(3,4,5)$ is scaled by $8$. Does the scaled triangle remain right?",
        "yes", "The sides are $(24,32,40)$. Scaling squares multiplies both sides by $8^2$: $576+1024=1600$."),
    ("pythagorean-converse/kp2", 1): (
        "Squares on a triangle have areas $4$, $9$, and $16$. Is there a right angle?",
        "no", "The longest side has square $16$, while $4+9=13$. The unequal totals rule out a right angle."),
    ("pythagorean-converse/kp2", 2): (
        "A frame has side lengths $5$, $6$, and $9$ cm. A builder claims the $5$ cm and $6$ cm sides are perpendicular. Is the claim correct?",
        "no", "Perpendicular sides would require $9^2=5^2+6^2$. But $81\\ne61$, so the claim fails."),
    ("pythagorean-converse/kp2", 3): (
        "A student tests sides $3$, $4$, $6$ using $3^2+6^2=4^2$. Is this the correct Pythagorean test?",
        "no", "The longest side is $6$, so the required comparison is $3^2+4^2=25$ against $6^2=36$."),
}

SKETCHES = {
    ("rational-exponents/kp1", 0):
        "$2\\cdot2\\cdot2=8$, so the cube root is $2$. Then square that root: $2\\cdot2=4$.",
    ("rational-exponents/kp1", 3):
        "$3\\cdot3=9$, so the principal square root is $3$. Cube it: $3\\cdot3\\cdot3=27$.",
}


def patch(text):
    """Replace only designated exemplar fields, preserving existing contracts."""
    topic = kp = None
    index = -1
    output = []
    seen = set()
    for line in text.splitlines(keepends=True):
        if match := re.match(r"^  - id: (\S+)", line):
            topic, kp = match[1], None
        if match := re.match(r"^      - id: (\S+)", line):
            kp, index = match[1], -1
        if line.startswith("          - problem:"):
            index += 1
        key = (f"{topic}/{kp}", index)
        if key in REPLACEMENTS:
            fields = zip(("problem", "answer", "solution_sketch"), REPLACEMENTS[key])
            for field, value in fields:
                prefix = "          - " if field == "problem" else "            "
                if line.startswith(prefix + field + ":"):
                    line = prefix + field + ": " + json.dumps(value, ensure_ascii=False) + "\n"
                    seen.add(key)
        elif key in SKETCHES and line.startswith("            solution_sketch:"):
            line = "            solution_sketch: " + json.dumps(SKETCHES[key]) + "\n"
            seen.add(key)
        output.append(line)
    assert seen == REPLACEMENTS.keys() | SKETCHES.keys(), seen
    return "".join(output)


if __name__ == "__main__":
    UNIT.write_text(patch(UNIT.read_text()))
