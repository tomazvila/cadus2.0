"""Worked steps for places where the original text only named a procedure."""


def revise(p):
    for key, sketches in ROWS.items():
        for index, sketch in enumerate(sketches):
            if sketch:
                p(key, index, sketch=sketch)


ROWS = {
    "polynomial-basics/kp2": [r"The nonzero terms are $3x^2$ and $-5x$. Two terms make a binomial.", None, None, None],
    "polynomial-multiplication/kp3": [None,
        r"Group as $((x^2+1)+x)((x^2+1)-x)=(x^2+1)^2-x^2=x^4+x^2+1$; the odd powers cancel.",
        r"The cubic terms cancel. Quadratic coefficients sum to $-1-1+2=0$; linear terms give $x+2x=3x$, and the constant is $-2$. The result is $x^4+3x-2$.",
        r"Collect the nine products by degree: cubic coefficients $-2+3=1$, quadratic coefficients $1-6-2=-7$, linear coefficients $3+4=7$, and constant $-2$."],
    "special-products/kp2": [r"The cross-products are $-7x$ and $7x$, so they cancel. The remaining terms are $x^2$ and $-49$.", None, None, None],
    "binomial-cubes/kp2": [r"First square to obtain $x^2-6x+9$. Multiply by $x-3$: the quadratic terms total $-9x^2$, linear terms total $27x$, and the constant is $-27$.", None, None, None],
    "dividing-polynomials-by-monomials/kp1": [
        r"The three quotients are $6x^3/(3x)=2x^2$, $-9x^2/(3x)=-3x$, and $3x/(3x)=1$. Add them.",
        r"Divide each term: $8x^4/(4x^2)=2x^2$ and $4x^2/(4x^2)=1$. The quotient is $2x^2+1$.",
        r"Length is area divided by width. The term quotients are $15x^4/(5x^2)=3x^2$ and $-5x^2/(5x^2)=-1$.",
        r"Divide coefficients and subtract the divisor's exponent: $6x^5/(3x^2)=2x^3$ and $-9x^3/(3x^2)=-3x$."],
    "dividing-polynomials-by-monomials/kp3": [
        r"The quotients are $9x^2y^3/(3xy)=3xy^2$ and $6xy^2/(3xy)=2y$, found by subtracting the exponents of each variable.",
        r"Divide $4a^3b$ by $2a^2b$ to get $2a$. Divide $-2a^2b^2$ by the same divisor to get $-b$. Add the two quotients.", None, None],
    "polynomial-division/kp1": [
        r"The leading quotient is $x$. Subtract $x(x+2)$ to leave $3x+6$; then divide by $x+2$ to get $3$. The quotient is $x+3$ with remainder zero.",
        r"The leading quotient is $x$. Subtract $x(x-3)$ to leave $-4x+12$; the next quotient term is $-4$, leaving no remainder.",
        r"Long division starts with $x$. Subtract $x(x-2)$ from the dividend to leave $3x-6$; dividing once more gives $3$ and zero remainder.",
        r"Length is the area divided by $x+4$. The first quotient term $x$ leaves $-5x-20$ after subtraction; the next term $-5$ leaves remainder zero."],
    "polynomial-division/kp3": [
        r"Use dividend coefficients $1,0,0,-8$. Subtract $x^2(x-2)$ to leave $2x^2-8$, then $2x(x-2)$ to leave $4x-8$, and finally $4(x-2)$. The quotient is $x^2+2x+4$.",
        r"Use $x^3+0x^2+0x+1$. Quotient terms $x^2$, $-x$, and $1$ successively leave remainders $-x^2+1$, $x+1$, and $0$.",
        r"Use $x^3+0x^2+0x-27$. Successive quotient terms $x^2$, $3x$, and $9$ leave $3x^2-27$, then $9x-27$, then zero.",
        r"Use $x^3+0x^2+0x+8$. The quotient terms $x^2$, $-2x$, and $4$ leave $-2x^2+8$, then $4x+8$, then zero."],
    "factoring-trinomials/kp3": [None,
        r"Extract $3$, leaving $2x^2-5x-3$. Split $-5x$ as $-6x+x$, group as $2x(x-3)+(x-3)$, and obtain $3(2x+1)(x-3)$.", None, None],
    "difference-of-squares/kp3": [r"Extract the coefficient GCF $2$, leaving $x^2-9=x^2-3^2$. The conjugate factors are $x-3$ and $x+3$.", None,
        r"The coefficients reconstruct $3x^2-27$. Extract $3$ to leave $x^2-9$; its square roots give factors $x-3$ and $x+3$.", None],
    "perfect-square-trinomials/kp2": [None,
        r"The constant is $36=6^2$ and the middle term is $-12x=-2(x)(6)$. These match the square $(x-6)^2$.", None, None],
    "sum-difference-of-cubes/kp3": [
        r"The cube roots are $2x$ and $3$. Use $a-b$ and $a^2+ab+b^2$ to obtain $(2x-3)(4x^2+6x+9)$.",
        r"The cube roots are $3x$ and $1$. A cube sum factors into $a+b$ and $a^2-ab+b^2$, giving $(3x+1)(9x^2-3x+1)$.",
        r"Reconstruct $8x^3-1=(2x)^3-1^3$. A cube difference gives $(2x-1)((2x)^2+2x+1)$.", None],
    "zero-product-property/kp1": [
        r"The factors give $x-3=0$ or $x+5=0$. Solving the linear equations gives $x=3$ or $x=-5$.",
        r"At an axis intersection the product is zero. The factors $x$ and $x-4$ give $x=0$ and $x=4$.",
        r"The second factor also can vanish: $x+2=0$ gives $-2$. Together with $x-6=0$, the full root set is $6,-2$.",
        r"The area product vanishes at $x=0$ or $x+9=0$. Both are allowed real parameters, giving $0,-9$."],
    "zero-product-property/kp3": [
        r"Set each variable factor to zero: $x=0$, $x-2=0$, or $x+7=0$. The roots are $0,2,-7$.", None,
        r"The factor $x$ contributes zero; cancelling it would lose that root. The other equations $x-3=0$ and $x+4=0$ give $3$ and $-4$.", None],
    "converting-to-vertex-form/kp3": [
        r"Add and subtract $25$: $y=(x+5)^2+30-25=(x+5)^2+5$. The square's minimum is zero, so the minimum output is $5$.", None,
        r"Add and subtract $36$: $y=(x+6)^2+40-36=(x+6)^2+4$. The square is nonnegative, so the minimum height is $4$.", None],
}
