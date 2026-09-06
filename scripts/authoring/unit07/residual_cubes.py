"""Cube identities with small roots and meaningful changes in both operands."""
from residual_recipes import recipe


def replacements():
    rows = [
        recipe("sum-difference-of-cubes/kp1",
               "Factor $(x+{a})^3-{b}$ over the integers using the difference-of-cubes identity.",
               "(x+a-d)*(x**2+(2*a+d)*x+a**2+a*d+d**2)",
               "The cube roots are $x+{a}$ and ${d}$. Their difference is $x+{a}-{d}$. The second factor is $(x+{a})^2+{d}(x+{a})+{d}^2$; expand this quadratic and keep it multiplied by the first factor.",
               {"a": [3, 4, 5], "d": [6, 7, 8, 9]}, {"b": "d**3"}),
        recipe("sum-difference-of-cubes/kp2",
               "Factor $(x-{a})^3+{b}$ over the integers using the sum-of-cubes identity.",
               "(x-a+d)*(x**2-(2*a+d)*x+a**2+a*d+d**2)",
               "The cube roots are $x-{a}$ and ${d}$. Their sum gives the linear factor. The quadratic is $(x-{a})^2-{d}(x-{a})+{d}^2$: its middle sign opposes the sign in the linear factor.",
               {"a": [3, 4, 5], "d": [6, 7, 8, 9]}, {"b": "d**3"}),
        recipe("sum-difference-of-cubes/kp3",
               "Factor ${q}x^3-{b}$ completely over the integers using the cube identity.",
               "(a*x-d)*(a**2*x**2+a*d*x+d**2)",
               "The cube roots of the full terms are ${a}x$ and ${d}$. Form their difference, then the quadratic $({a}x)^2+({a}x)({d})+{d}^2$. The coprime roots leave no coefficient GCF, and the quadratic's negative discriminant makes it irreducible.",
               {"a": [2, 3, 4], "d": [5, 7, 11, 13]}, {"q": "a**3", "b": "d**3"}),
    ]
    return {row["kp_id"]: row for row in rows}
