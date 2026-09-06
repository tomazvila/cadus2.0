"""Factoring contexts retain the required factorization and exact KP scope."""
import re
import sympy as s
import yaml
from revise import UNIT, expression


def representations(p):
    unit = yaml.safe_load(UNIT.read_text())
    prefixes = ("factoring-", "difference-of-squares", "perfect-square-trinomials", "sum-difference-of-cubes", "quadratics-in-form", "choosing-factoring-strategy")
    for topic in unit["topics"]:
        if not topic["id"].startswith(prefixes):
            continue
        for kp in topic["knowledge_points"]:
            key = topic["id"] + "/" + kp["id"]
            rows = kp["exemplars"]
            if key in ("perfect-square-trinomials/kp1", "choosing-factoring-strategy/kp1", "factoring-gcf/kp3"):
                continue
            source = re.search(r"\$(.*?)\$", rows[1]["problem"]).group(1)
            degree = s.degree(expression(rows[1]["answer"]), s.Symbol("x"))
            quantity = "area" if degree == 2 else "volume"
            p(key, 1, f"A design model has {quantity} ${source}$ for $x>20$. Write this model as a product of integer-coefficient polynomial factors" + (" by extracting its GCF." if topic["id"] == "factoring-gcf" else ", factoring completely over the integers."))
            poly = s.Poly(expression(rows[2]["answer"]), s.Symbol("x"))
            terms = "; ".join(f"power ${power[0]}$: coefficient ${coeff}$" for power, coeff in poly.terms())
            p(key, 2, f"A polynomial has the following nonzero coefficients: {terms}. Reconstruct it and write its factored form over the integers.")


def revise(p):
    representations(p)
    gcf(p)
    patterns(p)
    advanced(p)


def gcf(p):
    p("gcf-of-monomials/kp1", 1, r"Two prime-factor lists are $2\cdot3^2$ and $3^2\cdot5$. Find their greatest common factor.")
    p("gcf-of-monomials/kp1", 2, "A florist has $24$ roses and $36$ lilies. What is the greatest number of identical bouquets using every flower?", sketch=r"The bouquet count must divide both totals. Their prime factors share $2^2\cdot3=12$, so twelve bouquets are possible.")
    p("gcf-of-monomials/kp1", 3, "A student claims the GCF of $16$ and $40$ is $4$. Give the largest common factor.", sketch="Both numbers divide by $8$: $16=2(8)$ and $40=5(8)$. Since $2$ and $5$ are coprime, no larger common factor exists.")
    p("gcf-of-monomials/kp2", 1, "A monomial table lists coefficient/exponent pairs $(8,2)$ and $(20,1)$ for the variable $x$. Find the GCF monomial.")
    p("gcf-of-monomials/kp2", 2, "A student proposes $5x^4$ as the GCF of $15x^4$ and $25x^2$. Correct the monomial.")
    p("gcf-of-monomials/kp2", 3, "A common monomial factor must divide both terms of $9x^3+6x$. Find the greatest such monomial.")
    p("gcf-of-monomials/kp3", 2, "Exponent-table rows $(10,3,2)$ and $(15,1,4)$ list coefficient, $x$ exponent, and $y$ exponent. Find the GCF of the represented monomials.")
    p("gcf-of-monomials/kp3", 3, "A student proposes $4x^2y^2$ as the common monomial factor of $8x^2y$, $12xy^2$, and $20xy$. Correct the GCF.")
    p("factoring-gcf/kp1", 1, sketch="The coefficient GCF is $5$ and every term contains $x$. Dividing by $5x$ gives $2x^2$, $-3x$, and $1$, so extract $5x$.")
    p("factoring-gcf/kp1", 3, "A learner extracts $3x$ from $9x^3-6x^2+3x$ but writes $3x(3x^2-2x)$. Correct the factored expression.")
    p("factoring-gcf/kp2", 1, sketch="Coefficients share $7$ and the smallest exponent is $2$. Divide by $7x^2$: the remaining terms are $2x^2$, $3x$, and $-1$.")
    p("factoring-gcf/kp2", 2, "Extract a negative GCF from $-6x^2-9x$, leaving a positive leading coefficient inside the bracket.")
    p("factoring-gcf/kp2", 3, "A learner proposes $4x^2(3x^2-2x)$ for $12x^4-8x^3+4x^2$. Restore the missing term and give the factored expression.")
    p("factoring-gcf/kp3", 1, "Two adjacent rectangles share width $x-4$ and have lengths $x$ and $3$, with $x>4$. Their total area is $x(x-4)+3(x-4)$. Write it as one product.")
    p("factoring-gcf/kp3", 2, "A factor table lists two products: $4x$ times $(x+5)$, and $3$ times $(x+5)$. Factor their sum without expanding the common binomial.")
    p("factoring-gcf/kp3", 3, "Correct the incomplete extraction $2x(x-1)-5(x-1)=(x-1)(2x)$.", sketch="Both summands contain $(x-1)$. Their remaining factors are $2x$ and $-5$, so the second bracket is $2x-5$.")
    p("factoring-by-grouping/kp1", 3, "A learner groups $x^3-2x^2+7x-14$ as $x^2(x-2)+7(x-2)$ and stops. Complete the factorization.", sketch="Both groups contain $x-2$. Extract it to obtain $(x-2)(x^2+7)$; the quadratic has no integer factors.")
    p("factoring-by-grouping/kp2", 3, "Correct the grouped factorization $2x^3-5x^2+6x-15=(2x-5)(x^2+6)$.", sketch="The groups are $x^2(2x-5)$ and $3(2x-5)$. The second bracket must be $x^2+3$.")


def patterns(p):
    p("factoring-monic-trinomials/kp1", 3, "A learner proposes $(x+1)(x+21)$ for $x^2+10x+21$. Replace it with the correct factorization.", sketch="The proposed constants multiply to $21$ but add to $22$. The pair $3,7$ has product $21$ and sum $10$.")
    p("factoring-monic-trinomials/kp2", 3, "A learner proposes $(x-5)(x+8)$ for $x^2-3x-40$. Correct the signs in the factorization.", sketch="The constants must multiply to $-40$ and add to $-3$: use $-8$ and $5$.")
    p("factoring-monic-trinomials/kp3", 3, "Correct $(x+4)(x+5)$ as a factorization of $x^2-9x+20$.", sketch="The positive product and negative sum require two negative constants: $-4-5=-9$ and $(-4)(-5)=20$.")
    p("factoring-trinomials/kp1", 3, "The $ac$ method splits $3x^2+7x+2$ into $3x^2+6x+x+2$. Finish the factorization by grouping.", sketch="Group as $3x(x+2)+1(x+2)$, then extract the shared $x+2$.")
    p("factoring-trinomials/kp2", 3, "The middle term of $6x^2+5x-4$ is split as $8x-3x$. Finish the $ac$-method factorization.", sketch="Write $2x(3x+4)-1(3x+4)$ and extract $3x+4$.")
    p("factoring-trinomials/kp3", 3, "A learner stops at $2(2x^2+2x-12)$ while factoring $4x^2+4x-24$. Complete the factorization.", sketch="Extract the remaining $2$ to get $4(x^2+x-6)$. The constants $3,-2$ multiply to $-6$ and add to $1$.")
    p("difference-of-squares/kp1", 3, "A learner writes $x^2-100=(x-10)^2$. Correct the factorization.", "(x-10)*(x+10)", "The squares are $x^2$ and $10^2$. Opposite signs cancel the cross terms: $(x-10)(x+10)=x^2-100$.")
    p("difference-of-squares/kp2", 3, "Correct $(5x-2)(5x+2)$ as a factorization of $25x^4-4$.", sketch="The square root of $25x^4$ is $5x^2$. The conjugate factors are therefore $5x^2-2$ and $5x^2+2$.")
    p("difference-of-squares/kp3", 3, "A learner stops at $(x^2-9)(x^2+9)$ when factoring $x^4-81$. Complete the factorization over the integers.", sketch="The factor $x^2-9$ is another difference of squares, $(x-3)(x+3)$; $x^2+9$ remains irreducible over the integers.")
    p("perfect-square-trinomials/kp1", 2, "An area model has one $x^2$ square, two strips of area $4x$, and a corner of area $16$. Write the total area as a squared binomial.", sketch="The strips total $8x=2(x)(4)$ and the corner is $4^2$, so the assembled square has side $x+4$.")
    p("perfect-square-trinomials/kp1", 3, "A student calls $x^2+4x+16$ a perfect-square trinomial because its end terms are squares. Is the claim valid? Answer yes or no.", sketch="The end terms would require $a=x$ and $b=4$, so the middle term must be $2ab=8x$. The actual $4x$ fails that test.")
    p("perfect-square-trinomials/kp2", 3, "Correct $(x+8)^2$ as a factorization of $x^2-16x+64$.", sketch="Since $64=8^2$ and the middle term is $-2(x)(8)$, the binomial is $x-8$.")
    p("perfect-square-trinomials/kp3", 3, "Correct $(5x+4)^2$ as a factorization of $25x^2+20x+4$.", sketch="The end terms are $(5x)^2$ and $2^2$. The middle term $2(5x)(2)=20x$ confirms $(5x+2)^2$.")


def advanced(p):
    p("sum-difference-of-cubes/kp1", 3, "Correct $(x-5)(x^2-5x+25)$ as a factorization of $x^3-125$.", sketch="For a cube difference the quadratic cross term is positive: $x^2+5x+25$. Multiplication cancels the two middle powers.")
    p("sum-difference-of-cubes/kp2", 3, "Correct $(x+5)(x^2+5x+25)$ as a factorization of $x^3+125$.", sketch="For a cube sum the quadratic middle term has the opposite sign: $x^2-5x+25$.")
    p("sum-difference-of-cubes/kp3", 3, "The cube roots of $64x^3$ and $27$ are $4x$ and $3$. A learner uses $(4x+3)(16x^2+12x+9)$. Correct the sum-of-cubes factorization.", sketch="The linear factor is $4x+3$; the quadratic is $(4x)^2-(4x)(3)+3^2=16x^2-12x+9$.")
    p("quadratics-in-form/kp1", 3, "The substitution $u=x^2$ changes $x^4-8x^2+15$ into $(u-3)(u-5)$. Return to $x$ and give the integer factorization.", sketch="Replace each $u$ by $x^2$, obtaining $(x^2-3)(x^2-5)$. Neither constant is a perfect square, so neither factor splits over the integers.")
    p("quadratics-in-form/kp2", 1, "An area model has polynomial $x^4-13x^2+36$, with $x>3$. Use $u=x^2$, then differences of squares, to write the area as a product of integer linear factors.")
    p("quadratics-in-form/kp2", 3, "A learner stops at $(x^2-9)(x^2-16)$ for $x^4-25x^2+144$. Finish factoring every factor over the integers.", sketch="Each factor is a difference of squares: $x^2-9=(x-3)(x+3)$ and $x^2-16=(x-4)(x+4)$.")
    p("quadratics-in-form/kp3", 3, "The substitution $u=x^2$ gives $3u^2+7u+2=(3u+1)(u+2)$. Write the factorization of $3x^4+7x^2+2$ in $x$.", sketch="Replace $u$ with $x^2$ in both brackets: $(3x^2+1)(x^2+2)$; each quadratic remains irreducible over the integers.")
    p("choosing-factoring-strategy/kp1", 2, "For $3x^2-27$, first check a common coefficient, then the remaining two-term pattern. Write the complete factorization.", sketch="All coefficients share $3$, leaving $x^2-9$. Factor the difference of squares as $(x-3)(x+3)$.")
    p("choosing-factoring-strategy/kp1", 3, "A learner selects difference of squares for $x^2+20x+100$. Correct the choice by writing the polynomial in its matching factored pattern.", sketch="There are three terms and the middle one is $2(x)(10)$, so the perfect-square pattern gives $(x+10)^2$.")
    p("choosing-factoring-strategy/kp2", 3, "A learner extracts $2$ from $2x^2+20x+50$ and stops at $2(x^2+10x+25)$. Complete the factorization.", sketch="The inner trinomial has end terms $x^2$ and $5^2$ and middle term $2(x)(5)$, so it is $(x+5)^2$.")
    p("choosing-factoring-strategy/kp3", 3, "A learner groups $x^3+2x^2-9x-18$ into $(x+2)(x^2-9)$. Complete the integer factorization.", sketch="The second factor is $(x-3)(x+3)$. The complete product is $(x+2)(x-3)(x+3)$.")
