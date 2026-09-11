"""Recipe table for the advanced integers-negatives knowledge points."""
from __future__ import annotations

from fractions import Fraction

import foundations_curriculum_patch as curriculum
import integers_negatives_contracts as contracts
import integers_negatives_helpers as helpers

RECIPES: dict[curriculum.KpKey, list[curriculum.NewExemplar]] = {
    # -- ordering-rational-numbers ---------------------------------------------
    # constraint: "denominators up to 10; one decimal place; both values negative"
    curriculum.KpKey("ordering-rational-numbers", "kp1"): [
        helpers._compare_rationals(Fraction(-7, 10), "-0.7", Fraction(-2, 5), "-0.4"),
        # held-out: a fraction-vs-fraction pair (the existing two compare a
        # decimal pair and a fraction pair with denominator 2 vs 3; this
        # uses denominators 5 and 10, the constraint's own upper bound).
        helpers._compare_rationals(Fraction(-3, 5), "-\\frac{3}{5}", Fraction(-7, 10), "-\\frac{7}{10}"),
    ],
    # constraint: "three or four values mixing fractions and decimals; denominators up to 10"
    curriculum.KpKey("ordering-rational-numbers", "kp2"): [
        helpers._order_rationals(
            [Fraction(-9, 10), Fraction(1, 4), Fraction(-1, 2), Fraction(3, 10)],
            ["-0.9", "\\frac{1}{4}", "-\\frac{1}{2}", "0.3"],
        ),
        # held-out: all four values negative (the existing two each mix in a
        # positive value).
        helpers._order_rationals(
            [Fraction(-1, 4), Fraction(-9, 10), Fraction(-3, 5), Fraction(-1, 10)],
            ["-\\frac{1}{4}", "-0.9", "-\\frac{3}{5}", "-0.1"],
        ),
    ],
    # -- signed-decimal-operations ---------------------------------------------
    # constraint: "1 decimal place; values from -10 to 10" (already has 3)
    curriculum.KpKey("signed-decimal-operations", "kp1"): [
        # held-out: both addends negative (the existing three: neg+pos,
        # neg-pos, pos-pos — none has BOTH values negative).
        helpers._decimal_add(Fraction(-42, 10), Fraction(-15, 10)),
    ],
    # constraint: "1 decimal place; exact quotients; values from -10 to 10"
    curriculum.KpKey("signed-decimal-operations", "kp2"): [
        helpers._decimal_muldiv(Fraction(-72, 10), "/", Fraction(-8, 10)),
        # held-out: multiplying two negatives (the existing two are a
        # negative-times-positive and a negative-divided-by-negative).
        helpers._decimal_muldiv(Fraction(-25, 10), "*", Fraction(-4, 10)),
    ],
    # -- adding-subtracting-negative-fractions ---------------------------------
    # constraint: "one denominator divides the other; denominators up to 12"
    curriculum.KpKey("adding-subtracting-negative-fractions", "kp1"): [
        helpers._frac_add(Fraction(-3, 8), "-", Fraction(1, 4)),
        # held-out: adding two negative fractions (the existing two each mix
        # a negative with a positive fraction).
        helpers._frac_add(Fraction(-5, 12), "+", Fraction(-1, 4)),
    ],
    # constraint: "denominators up to 12; answers in lowest terms"
    curriculum.KpKey("adding-subtracting-negative-fractions", "kp2"): [
        helpers._frac_add(Fraction(1, 4), "-", Fraction(5, 6)),
        # held-out: two negative fractions needing an LCD (the existing two
        # are positive-minus-positive and negative-plus-positive).
        helpers._frac_add(Fraction(-5, 8), "-", Fraction(-1, 6)),
    ],
    # -- negative-fractions-decimals --------------------------------------------
    # constraint: "denominators up to 12; answers in lowest terms" (already has 3)
    curriculum.KpKey("negative-fractions-decimals", "kp1"): [
        # held-out: dividing two negative fractions (the existing three:
        # negative-times-positive, negative-divided-by-positive,
        # negative-times-negative — none divides two negatives).
        helpers._frac_muldiv(Fraction(-3, 4), "/", Fraction(-9, 8)),
    ],
    # constraint: "convert to one form first; denominators up to 10"
    curriculum.KpKey("negative-fractions-decimals", "kp2"): [
        helpers._frac_add(Fraction(-3, 4), "-", Fraction(1, 5)),
        # held-out: multiplying two negative decimal/fraction quantities (the
        # existing two are a signed sum and a negative-times-positive
        # product).
        helpers._exact(
            "Compute $-0.4 \\times \\left(-\\frac{5}{8}\\right)$. Give the answer as a fraction.",
            helpers._frac_answer(Fraction(-4, 10) * Fraction(-5, 8)),
            f"Two negatives: positive; $\\frac{{4}}{{10}} \\times \\frac{{5}}{{8}} = {helpers._frac_str(Fraction(-4,10)*Fraction(-5,8))}$.",
        ),
    ],
    # constraint: "convert to improper fractions first; denominators up to 12"
    curriculum.KpKey("negative-fractions-decimals", "kp3"): [
        helpers._mixed_add(-1, 1, 4, "-", 1, 1, 2),
        # held-out: both mixed numbers negative (the existing two each keep
        # one term positive).
        helpers._mixed_add(-2, 1, 2, "+", -1, 1, 4),
    ],
    # -- exponent-notation ------------------------------------------------------
    # constraint: "base 2-6, exponent 2-4"
    curriculum.KpKey("exponent-notation", "kp1"): [
        helpers._exact("Compute $5^3$.", str(5**3), f"$5 \\times 5 \\times 5 = {5**3}$."),
        # held-out: writing a product of FIVE equal factors in exponent form
        # (the existing two write a product of three, or evaluate a power) —
        # the reverse direction of the KP's own first exemplar, at the
        # constraint's higher exponent.
        helpers._exact(
            "Write $6 \\times 6 \\times 6 \\times 6$ in exponent form.",
            "6^4",
            "Four equal factors of $6$: $6^4$.",
        ),
    ],
    # constraint: "base 2-10, exponent 2-4; products at most 10000" (already has 3)
    curriculum.KpKey("exponent-notation", "kp2"): [
        # held-out: exponent 4 with a small base (the existing three use
        # exponents 2, 3, and 4 but the exponent-4 case is base 10; this is
        # base 2, testing the same exponent at the opposite end of the base
        # range).
        helpers._exact("Compute $2^4$.", str(2**4), f"$2 \\times 2 \\times 2 \\times 2 = {2**4}$."),
    ],
    # -- integer-exponents-intro --------------------------------------------------
    # constraint: "base -4 to -2, exponent 2-4; base in parentheses"
    curriculum.KpKey("integer-exponents-intro", "kp1"): [
        helpers._neg_base_power(4, 2),
        # held-out: an odd exponent at the constraint's largest base
        # magnitude (the existing two use exponents 2 and 3 at bases 3 and
        # 2; this reaches exponent 4 at base 4).
        helpers._neg_base_power(4, 4),
    ],
    # constraint: "pair -a^n with (-a)^n; base 2-4, exponent 2-4"
    curriculum.KpKey("integer-exponents-intro", "kp2"): [
        helpers._neg_pow_no_parens(3, 3),
        # held-out: the parenthesized counterpart of the same base/exponent,
        # completing the pair the KP's own name promises and giving the
        # OPPOSITE sign of the new unparenthesized exemplar above.
        helpers._neg_base_power(3, 3),
    ],
    # constraint: "exponents mixed with one or two other operations" (already has 3)
    curriculum.KpKey("integer-exponents-intro", "kp3"): [
        # held-out: a negative base power added to a positive-base power
        # (the existing three: sum of two positive powers, a power of a
        # parenthesized negative, a product with a squared negative — none
        # sums a negative-base power with another term).
        helpers._exact(
            "Compute $(-3)^3 + 5^2$.",
            str((-3) ** 3 + 5**2),
            f"$(-3)^3 = -27$; $5^2 = 25$; $-27 + 25 = {(-3)**3 + 5**2}$.",
        ),
    ],
    # -- temperature-elevation-problems ------------------------------------------
    # constraint: "single-step signed addition/subtraction; values from -60 to 60"
    curriculum.KpKey("temperature-elevation-problems", "kp1"): [
        helpers._single_change(
            "The temperature was $-12\\text{ C}$ and dropped $7\\text{ C}$. What is the new temperature in C?",
            -12,
            -7,
        ),
        # held-out: an elevation GAIN from a positive starting height (the
        # existing two both start below zero).
        helpers._single_change(
            "A hiker at $40\\text{ m}$ elevation climbs another $25\\text{ m}$. What is the new elevation in meters?",
            40,
            25,
        ),
    ],
    # constraint: "difference of two signed levels; subtract end minus start"
    curriculum.KpKey("temperature-elevation-problems", "kp2"): [
        helpers._gap(
            "A weather balloon rose from $-15\\text{ m}$ to $60\\text{ m}$. By how many metres did it rise?",
            60,
            -15,
        ),
        # held-out: both levels negative (the existing two each keep one
        # level at or above zero).
        helpers._gap(
            "The temperature fell from $-3\\text{ C}$ to $-18\\text{ C}$. By how many degrees did it fall?",
            -3,
            -18,
        ),
    ],
    # -- integer-word-problems ------------------------------------------------
    # constraint: "two or three signed steps"
    curriculum.KpKey("integer-word-problems", "kp1"): [
        helpers._net_change(
            "A hot air balloon at $120$ m descends $45$ m, then descends another $30$ m. What is its height in meters?",
            120,
            [-45, -30],
        ),
        # held-out: three steps with a rise between two descents (the
        # existing exemplars are a single withdrawal and a gain-loss-gain
        # sequence that nets to $0$; this nets to a negative value the other
        # two never reach).
        helpers._net_change(
            "A submarine at $-40$ m rises $25$ m, then descends $60$ m. What is its depth in meters?",
            -40,
            [25, -60],
        ),
    ],
    # constraint: "one multiplication of a signed rate by a count, plus a start value"
    curriculum.KpKey("integer-word-problems", "kp2"): [
        helpers._repeated_change(
            "A stock starts at $\\$80$ and loses $\\$15$ each of the next $3$ trading days. What is its value in dollars?",
            80,
            -15,
            3,
        ),
        # held-out: a POSITIVE repeated rate applied to a NEGATIVE starting
        # value (the existing exemplars both apply a negative rate to a
        # positive start).
        helpers._repeated_change(
            "A hiker starts at $-20$ m elevation and climbs $8$ m every $10$ minutes for $50$ minutes. What is the new elevation in meters after $50$ minutes?",
            -20,
            8,
            5,
        ),
    ],
}


# comparing-integers is handled entirely through CONTRACT_FIXES plus these two
# extra recipes (its two existing rows are fixed in place, not replaced).
RECIPES[curriculum.KpKey("comparing-integers", "kp1")] = [
    helpers._exact(
        "Which symbol, $<$ or $>$, makes $-9 \\;\\square\\; -1$ true?",
        "<",
        "$-9$ is left of $-1$ on the number line, so $-9 < -1$.",
        contract=contracts.LABEL_LT_GT,
    ),
    # held-out: a comparison against zero, a sub-case neither existing
    # (negative-negative, positive-negative) pair reaches.
    helpers._exact(
        "Insert $<$ or $>$ to make a true statement: $0 \\;\\square\\; -6$.",
        ">",
        "$0$ is right of $-6$ on the number line, so $0 > -6$.",
        contract=contracts.LABEL_LT_GT,
    ),
]
RECIPES[curriculum.KpKey("comparing-integers", "kp2")] = [
    helpers._truth(-3, 2, "<"),
    # held-out: an all-negative three-term chain (the existing, now-fixed
    # chain mixes signs).
    helpers._exact(
        "Arrange $-2$, $-8$, $-5$ in increasing order using $<$.",
        "-8 < -5 < -2",
        "On the number line the increasing order is -8 < -5 < -2.",
        contract=contracts.ASCENDING_CHAIN,
    ),
]
