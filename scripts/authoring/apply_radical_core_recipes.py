#!/usr/bin/env python3
"""Apply five explicit, hand-verified radical knowledge-point recipes.

Each recipe names one exact knowledge point and uses only perfect powers in
that point's authored range. The solution sketches expose the root/power
identity used for the answer. The generic Foundations generators remain
fail-closed; this script changes only the named rows below.

Default is a dry run. Pass ``--write`` to update the unit YAML in place.
"""
from __future__ import annotations

from foundations_curriculum_patch import (
    ExemplarKey,
    KpKey,
    NewExemplar,
)
from foundations_recipe_cli import run_recipe_cli

UNIT_FILE = "curriculum/foundations/06-exponents-radicals.yaml"

MISSING_SKETCHES: dict[ExemplarKey, str] = {
    ExemplarKey("perfect-square-roots", "kp2", 0):
        "$7^2 = 49$, so the principal square root is $7$.",
    ExemplarKey("perfect-square-roots", "kp2", 1):
        "$11^2 = 121$, so the principal square root is $11$.",
    ExemplarKey("perfect-square-roots", "kp3", 1):
        "$15^2 = 225$, so the principal square root is $15$.",
    ExemplarKey("cube-roots", "kp2", 1):
        "$(-5)^3 = -125$; an odd root preserves the negative sign.",
}


def _exact(problem: str, answer: str, sketch: str) -> NewExemplar:
    return NewExemplar(problem, answer, sketch, with_contract=True)


RECIPES: dict[KpKey, list[NewExemplar]] = {
    KpKey("perfect-square-roots", "kp2"): [
        _exact(
            "Compute $\\sqrt{100}$.",
            "10",
            "$10^2 = 100$, so the principal square root is $10$.",
        ),
        _exact(
            "Compute $\\sqrt{81}$.",
            "9",
            "$9^2 = 81$, so the principal square root is $9$.",
        ),
    ],
    KpKey("perfect-square-roots", "kp3"): [
        _exact(
            "Compute $\\sqrt{361}$.",
            "19",
            "$19^2 = 361$, so the principal square root is $19$.",
        ),
        _exact(
            "Compute $\\sqrt{324}$.",
            "18",
            "$18^2 = 324$, so the principal square root is $18$.",
        ),
    ],
    KpKey("square-roots", "kp1"): [
        _exact(
            "Compute $2\\sqrt{64}$.",
            "16",
            "$\\sqrt{64} = 8$, then $2 \\cdot 8 = 16$.",
        ),
        _exact(
            "Compute $5\\sqrt{9}$.",
            "15",
            "$\\sqrt{9} = 3$, then $5 \\cdot 3 = 15$.",
        ),
    ],
    KpKey("square-roots", "kp3"): [
        _exact(
            "Compute $\\sqrt{9 \\cdot 16}$.",
            "12",
            "$\\sqrt{9 \\cdot 16} = \\sqrt{144} = 12$.",
        ),
        _exact(
            "Compute $\\sqrt{4 \\cdot 25}$.",
            "10",
            "$\\sqrt{4 \\cdot 25} = \\sqrt{100} = 10$.",
        ),
    ],
    KpKey("cube-roots", "kp2"): [
        _exact(
            "Compute $\\sqrt[3]{-64}$.",
            "-4",
            "$(-4)^3 = -64$; an odd root preserves the negative sign.",
        ),
        _exact(
            "Compute $\\sqrt[3]{-27}$.",
            "-3",
            "$(-3)^3 = -27$; an odd root preserves the negative sign.",
        ),
    ],
}


def main() -> int:
    return run_recipe_cli(
        UNIT_FILE,MISSING_SKETCHES,RECIPES,
        lambda sketched,applied,count,verb:
            f"{sketched} sketches and {count} exemplars across {applied} KPs {verb}",
        description=__doc__,
    )


if __name__ == "__main__":
    raise SystemExit(main())
