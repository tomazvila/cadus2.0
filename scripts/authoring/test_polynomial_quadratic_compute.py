"""Independent checks of the exact polynomial/quadratic arithmetic."""
import unittest
from fractions import Fraction as F

import polynomial_quadratic_compute as pc


class PolynomialArithmeticTest(unittest.TestCase):
    def test_add_sub_mul_match_hand_expansion(self):
        self.assertEqual(pc.add(pc.poly(9, -3, 3), pc.poly(-6, 4, 1)), pc.poly(3, 1, 4))
        self.assertEqual(pc.sub(pc.poly(6, -1, 2), pc.poly(-4, 3, 1)), pc.poly(10, -4, 1))
        self.assertEqual(pc.mul(pc.poly(5, 1), pc.poly(3, 1)), pc.poly(15, 8, 1))

    def test_mul_many_matches_binomial_theorem(self):
        cube = pc.mul_many(pc.linear(-1), pc.linear(-1), pc.linear(-1))
        self.assertEqual(cube, pc.poly(1, 3, 3, 1))  # (x + 1)^3

    def test_divmod_exact_and_with_remainder(self):
        quotient, remainder = pc.divmod_poly(pc.poly(6, 5, 1), pc.poly(2, 1))
        self.assertEqual((quotient, remainder), (pc.poly(3, 1), pc.poly(0)))
        quotient, remainder = pc.divmod_poly(pc.poly(5, 3, 1), pc.poly(1, 1))
        self.assertEqual((quotient, remainder), (pc.poly(2, 1), pc.poly(3)))

    def test_synthetic_division_matches_long_division(self):
        quotient, remainder = pc.synthetic_division([1, -6, 11, -6], F(1))
        self.assertEqual((quotient, remainder), ([F(1), F(-5), F(6)], F(0)))
        long_q, long_r = pc.divmod_poly(pc.poly(-6, 11, -6, 1), pc.poly(-1, 1))
        self.assertEqual(long_q, pc.poly(*reversed(quotient)))
        self.assertEqual(long_r, pc.poly(remainder))

    def test_evaluate_is_horner_consistent_with_direct_substitution(self):
        # p(x) = x^2 - 4x + 7 at x = 3: 9 - 12 + 7 = 4
        self.assertEqual(pc.evaluate([1, -4, 7], F(3)), F(4))


class QuadraticRootsTest(unittest.TestCase):
    def test_rational_roots_reproduce_the_polynomial(self):
        cases = [(1, 7, 12), (1, 2, -15), (2, -5, -3), (3, -10, 8), (6, 1, -2)]
        for a, b, c in cases:
            with self.subTest(a=a, b=b, c=c):
                roots = pc.quadratic_roots(a, b, c)
                self.assertIsNotNone(roots.rational)
                r1, r2 = roots.rational
                rebuilt = pc.mul_many(pc.linear(r1, a), pc.linear(r2, 1))
                self.assertEqual(rebuilt, pc.trim(pc.poly(c, b, a)))

    def test_irrational_roots_reproduce_the_discriminant(self):
        for a, b, c in [(1, 2, -4), (1, -8, 5), (2, 4, -3), (2, 6, 1)]:
            with self.subTest(a=a, b=b, c=c):
                roots = pc.quadratic_roots(a, b, c)
                self.assertIsNotNone(roots.irrational)
                p, q, d = roots.irrational
                # substitute x = p + q*sqrt(d) into a x^2 + b x + c using
                # sqrt(d)^2 = d; the rational and irrational parts must both
                # vanish independently.
                rational_part = a * (p * p + q * q * d) + b * p + c
                irrational_coeff = a * 2 * p * q + b * q
                self.assertEqual(rational_part, 0)
                self.assertEqual(irrational_coeff, 0)

    def test_negative_discriminant_has_no_real_roots(self):
        roots = pc.quadratic_roots(1, 2, 5)
        self.assertLess(roots.discriminant, 0)
        self.assertIsNone(roots.rational)
        self.assertIsNone(roots.irrational)

    def test_squarefree_extraction_is_exact(self):
        for n, expected in [(12, (2, 3)), (20, (2, 5)), (72, (6, 2)), (7, (1, 7)), (49, (7, 1))]:
            with self.subTest(n=n):
                q, d = pc.squarefree(n)
                self.assertEqual((q, d), expected)
                self.assertEqual(q * q * d, n)


class VertexTest(unittest.TestCase):
    def test_vertex_matches_completing_the_square(self):
        for a, b, c in [(1, -6, 5), (2, -8, 3), (-1, 2, 3), (3, 6, 1)]:
            with self.subTest(a=a, b=b, c=c):
                h, k = pc.vertex(a, b, c)
                self.assertTrue(pc.check_vertex_form(a, b, c, h, k))


if __name__ == "__main__":
    unittest.main()
