"""Improve retained U07 worked examples, preserving all unrelated YAML bytes."""
import json
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[3]
PATH = ROOT / "curriculum/foundations/07-polynomials-quadratics.yaml"

SKETCHES = {
    "factoring-gcf/kp2": [
        "The coefficient GCF is $4$, and both terms contain $x$. Divide by $-4x$ to obtain $x+2$; the outside negative sign makes the inside leading coefficient positive.",
        "The coefficient GCF is $7$ and the lowest power is $x^2$. Dividing each term by $7x^2$ gives $2x^2+3x-1$, so the model is $7x^2(2x^2+3x-1)$.",
        "Divide by the negative GCF $-3x^2$. The quotients are $2x^2$, $-3x$, and $1$. The last term equals the GCF and therefore leaves a unit term.",
        "Divide all three terms by $4x^2$. The quotients are $3x^2$, $-2x$, and $1$; the missing unit term restores $4x^2$ when the product is expanded.",
    ],
    "difference-of-squares/kp2": [
        "The two terms are $(2x)^2$ and $5^2$. Their difference is $(2x-5)(2x+5)$; opposite cross terms cancel.",
        "The square root of $9x^4$ is $3x^2$, while the square root of $16$ is $4$. Use $(3x^2-4)(3x^2+4)$. Neither quadratic has rational roots, so both remain integer-irreducible.",
        "The coefficient table gives $16x^2-9=(4x)^2-3^2$. Taking the difference and sum of the square roots gives $(4x-3)(4x+3)$.",
        "The square root of $25x^4$ is $5x^2$. Thus the factors are $5x^2-2$ and $5x^2+2$. Both quadratics have no rational roots and remain irreducible over the integers.",
    ],
    "difference-of-squares/kp3": [
        "Extract the coefficient GCF $2$, leaving $x^2-9$. Its square roots are $x$ and $3$, so the complete product is $2(x-3)(x+3)$.",
        "Write $x^4-16=(x^2-4)(x^2+4)$. Split $x^2-4$ again into $(x-2)(x+2)$; the positive sum $x^2+4$ has no real roots and stays irreducible.",
        "Extract $3$ to leave $x^4-16$. Split it into $(x^2-4)(x^2+4)$, then split the first factor into $(x-2)(x+2)$. Retain the GCF and the irreducible sum of squares.",
        "The remaining $x^2-9$ is $(x-3)(x+3)$. Keep $x^2+9$ unchanged: it has no real roots and therefore no integer linear factors.",
    ],
    "perfect-square-trinomials/kp3": [
        "The end-term square roots are $2x$ and $3$. Their doubled product is $12x$, so the negative middle sign gives $(2x-3)^2$.",
        "The square roots are $3x$ and $5$. Twice their product is $30x$, exactly the middle term; hence the full trinomial is $(3x+5)^2$.",
        "The coefficient table reconstructs $16x^2-24x+9$. The end-term roots $4x$ and $3$ have doubled product $24x$; the negative middle sign gives $(4x-3)^2$.",
        "The end terms are $(5x)^2$ and $2^2$. Their doubled product is $20x$, confirming $(5x+2)^2$. A constant of $4$ inside the binomial would produce the wrong cross term and constant.",
    ],
    "quadratics-in-form/kp2": [
        "Set $u=x^2$. Then $u^2-5u+4=(u-1)(u-4)$. Restoring $x^2$ yields two differences of squares, giving $(x-1)(x+1)(x-2)(x+2)$.",
        "With $u=x^2$, the numbers $-4$ and $-9$ have sum $-13$ and product $36$. Thus $(x^2-4)(x^2-9)$ splits into the four linear factors $x-2,x+2,x-3,x+3$.",
        "The table gives $x^4-17x^2+16$. In $u=x^2$, factor as $(u-1)(u-16)$. Both restored quadratics split, using square roots $1$ and $4$.",
        "The first factor splits as $(x-3)(x+3)$ and the second as $(x-4)(x+4)$. All factors are now linear, so integer factorization is complete.",
    ],
    "choosing-factoring-strategy/kp2": [
        "Extract $2x$, leaving $x^2-4$. Its difference-of-squares pattern gives $(x-2)(x+2)$, and the outside $2x$ remains.",
        "Extract the GCF $3$. The inner trinomial $x^2+4x+4$ has end roots $x,2$ and middle term $2(x)(2)$, so it becomes $(x+2)^2$.",
        "Reconstruct $3x^3-12x$ and extract $3x$. The remaining $x^2-4$ splits into $x-2$ and $x+2$; keep all three factors.",
        "After extracting $2$, recognize $x^2+10x+25$: the constant is $5^2$ and the middle term is $2(x)(5)$. Replace that trinomial by $(x+5)^2$.",
    ],
    "choosing-factoring-strategy/kp3": [
        "First extract $2x$, leaving $x^3+4x^2-9x-36$. Group to obtain $(x+4)(x^2-9)$, then split the difference of squares into $x-3$ and $x+3$.",
        "Group the first two and last two terms as $x^2(x+3)-4(x+3)$. Extract $x+3$, then factor $x^2-4=(x-2)(x+2)$.",
        "The table gives $3x^3+6x^2-12x-24$. Extract $3$ and group the remainder as $(x+2)(x^2-4)$. Splitting $x^2-4$ produces the repeated factor $(x+2)^2$ and $x-2$.",
        "The factor $x^2-9$ still splits as $(x-3)(x+3)$. Keep the previously extracted $x+2$ to obtain the complete three-factor product.",
    ],
}
EDITS = {
    ("factoring-gcf/kp2", 2): {
        "problem": "Extract a negative GCF from $-6x^4+9x^3-3x^2$, leaving a positive leading coefficient inside the bracket.",
        "answer": "-3*x^2*(2*x^2-3*x+1)",
    },
    ("difference-of-squares/kp3", 2): {
        "problem": "A polynomial has nonzero coefficients $3$ at power $4$ and $-48$ at power $0$. Reconstruct it and factor completely over the integers.",
        "answer": "3*(x-2)*(x+2)*(x^2+4)",
    },
    ("choosing-factoring-strategy/kp3", 0): {
        "problem": "Factor $2x^4+8x^3-18x^2-72x$ completely over the integers.",
        "answer": "2*x*(x+4)*(x-3)*(x+3)",
    },
    ("choosing-factoring-strategy/kp3", 2): {
        "problem": "A polynomial has coefficients $(3,6,-12,-24)$ in descending powers from $x^3$. Reconstruct it and factor completely over the integers.",
        "answer": "3*(x+2)^2*(x-2)",
    },
}


from residual_authored_more import SKETCHES as MORE_SKETCHES, EDITS as MORE_EDITS
SKETCHES.update(MORE_SKETCHES)
EDITS.update(MORE_EDITS)


def fields(node):
    return {key.value: value for key, value in node.value}


def apply():
    from parameter_authored import updates as parameter_updates
    source = PATH.read_text()
    tree = fields(yaml.compose(source))
    changes = []
    for topic in tree["topics"].value:
        t = fields(topic)
        for kp in t["knowledge_points"].value:
            k = fields(kp)
            key = t["id"].value + "/" + k["id"].value
            for index, exemplar in enumerate(k["exemplars"].value):
                updates = dict(EDITS.get((key, index), {}))
                if key in SKETCHES:
                    updates["solution_sketch"] = SKETCHES[key][index]
                existing = {name: node.value for name, node in fields(exemplar).items()}
                updates.update(parameter_updates(key, index, existing))
                for name, value in updates.items():
                    node = fields(exemplar)[name]
                    changes.append((node.start_mark.index, node.end_mark.index, json.dumps(value)))
    for start, end, value in sorted(changes, reverse=True):
        source = source[:start] + value + source[end:]
    PATH.write_text(source)
    print(f"Updated {len(changes)} U07 authored fields")


if __name__ == "__main__":
    apply()
