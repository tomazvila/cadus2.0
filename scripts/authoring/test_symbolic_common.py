"""Tests of `symbolic_common`'s exact arithmetic and formatting."""
import unittest
from fractions import Fraction as F

from symbolic_common import (
    coordinates_contract,
    frac_answer,
    label_contract,
    slope,
    solve_2x2,
    solve_linear,
    tex_frac,
    unit_contract,
)


class FormattingTest(unittest.TestCase):
    def test_frac_answer_integer(self):
        self.assertEqual(frac_answer(F(6)), "6")
        self.assertEqual(frac_answer(F(-6)), "-6")

    def test_frac_answer_fraction(self):
        self.assertEqual(frac_answer(F(7, 2)), "7/2")
        self.assertEqual(frac_answer(F(-1, 3)), "-1/3")

    def test_tex_frac_integer(self):
        self.assertEqual(tex_frac(F(-4)), "-4")

    def test_tex_frac_fraction(self):
        self.assertEqual(tex_frac(F(3, 2)), "\\frac{3}{2}")
        self.assertEqual(tex_frac(F(-3, 2)), "-\\frac{3}{2}")


class SolveTest(unittest.TestCase):
    def test_solve_linear(self):
        self.assertEqual(solve_linear(2, 3, 11), F(4))
        self.assertEqual(solve_linear(3, 0, 7), F(7, 3))

    def test_solve_linear_rejects_zero_leading_coefficient(self):
        with self.assertRaises(ValueError):
            solve_linear(0, 1, 2)

    def test_solve_2x2(self):
        # x + y = 7, x - y = 3 -> (5, 2)
        self.assertEqual(solve_2x2(1, 1, 7, 1, -1, 3), (F(5), F(2)))

    def test_solve_2x2_rejects_singular_system(self):
        with self.assertRaises(ValueError):
            solve_2x2(1, 2, 3, 2, 4, 6)

    def test_slope(self):
        self.assertEqual(slope(1, 2, 4, 8), F(2))
        self.assertEqual(slope(-1, 4, 2, -5), F(-3))

    def test_slope_rejects_vertical_pair(self):
        with self.assertRaises(ValueError):
            slope(3, 1, 3, 6)


class ContractTest(unittest.TestCase):
    def test_label_contract_two_groups(self):
        self.assertEqual(
            label_contract("yes", "no"), '{"kind":"label","options":[["yes"],["no"]]}'
        )

    def test_label_contract_single_group_list(self):
        self.assertEqual(
            label_contract(["no solution"]), '{"kind":"label","options":[["no solution"]]}'
        )

    def test_unit_contract(self):
        self.assertEqual(
            unit_contract("volume", "L"), '{"kind":"unit","quantity":"volume","unit":"L"}'
        )

    def test_coordinates_contract(self):
        self.assertEqual(coordinates_contract(2), '{"kind":"coordinates","arity":2}')


if __name__ == "__main__":
    unittest.main()
