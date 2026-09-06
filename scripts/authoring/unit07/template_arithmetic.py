"""Template methods for polynomial arithmetic and division."""


def build(a):
    basics(a)
    sums(a)
    products(a)
    divisions(a)


def basics(a):
    a("polynomial-basics/kp1", "A polynomial is $p(x)={a}x^4+2x^2-7$. Report (degree, leading coefficient) in that order.", "(4,a)", "The highest nonzero term is ${a}x^4$. Its exponent is $4$ and its coefficient is ${a}$.", "Read the exponent and coefficient of the highest-power term.")
    a("polynomial-basics/kp2", "Classify ${a}x^2+3x+1$ as monomial, binomial, or trinomial.", "trinomial", "There are three nonzero powers: quadratic, linear, and constant. Three terms form a trinomial.", "Count nonzero like-term groups.", contract={"kind":"label","options":[["monomial"],["binomial"],["trinomial"]]})
    a("polynomial-basics/kp3", "A coefficient inventory contains $5$, $-2x$, and ${a}x^3$. Write the polynomial in descending powers.", "a*x**3-2*x+5", "Order the powers $3,1,0$: the cubic term comes first, then the linear term, then the constant.", "Order terms by exponent, keeping each sign attached.")
    a("evaluating-polynomials/kp1", "Find the output at input $3$ for $p(x)=x^2+{a}x-1$.", "8+3*a", "Substitution gives $3^2+{a}(3)-1=9+3({a})-1$; combine the constant terms.", "Square the input before multiplying the linear term.")
    a("evaluating-polynomials/kp2", "Find the output at input $-2$ for $p(x)=x^2+{a}x-1$.", "3-2*a", "Parentheses preserve the negative input: $(-2)^2+{a}(-2)-1=4-2({a})-1$.", "Even powers of a negative input are positive.")
    a("evaluating-polynomials/kp3", "A production model is $C=n^2+{a}n+2$. Find $C$ for $n=3$.", "11+3*a", "The model gives $3^2+{a}(3)+2=9+3({a})+2$.", "Replace each occurrence of the input with the stated production count.")


def sums(a):
    a("adding-polynomials/kp1", "Two path lengths are ${a}x+3$ and $2x+5$. Write their combined length in standard form.", "(a+2)*x+8", "The $x$ coefficients add to ${a}+2$, and the constants add to $3+5=8$.", "Group the linear terms separately from the constants.")
    a("adding-polynomials/kp2", "Combine area polynomials ${a}x^2+3x+1$ and $2x^2-x+4$ into one standard-form polynomial.", "(a+2)*x**2+2*x+5", "The quadratic coefficients add to ${a}+2$; $3x-x=2x$ and $1+4=5$.", "Only terms with equal powers can be combined.")
    a("adding-polynomials/kp3", "Add $({a}x^3+2x)+(x^2-2x+7)$, filling missing-power columns with zero.", "a*x**3+x**2+7", "The linear terms cancel: $2x-2x=0$. The cubic, quadratic, and constant terms occupy separate columns.", "Align powers before adding coefficient columns.")
    a("polynomial-addition-subtraction/kp1", "Subtract $(2x^2+3x-4)$ from $({a}x^2-x+5)$.", "(a-2)*x**2-4*x+9", "Distributing the minus gives $-2x^2-3x+4$. The coefficients then give $({a}-2)x^2-4x+9$.", "Change the sign of every term in the subtracted polynomial.", axes={"a":range(3,15)})
    a("polynomial-addition-subtraction/kp2", "Subtract $(x^3-x+2)$ from $(x^3+{a}x^2+4)$ and write the result in standard form.", "a*x**2+x+2", "The cubic terms cancel; distributing the subtraction changes $-x$ to $+x$, and the constant difference is $2$.", "Use zero coefficients for missing powers, then distribute the subtraction.")
    a("polynomial-addition-subtraction/kp3", "A stock model starts with $x^2+{a}x+4$, gains $2x^2+3$, and loses $x^2-2x+1$. Write the final polynomial.", "2*x**2+(a+2)*x+6", "The square coefficients give $1+2-1=2$; the linear coefficients give ${a}-(-2)$; constants give $4+3-1=6$.", "Keep the entire loss polynomial inside the subtraction until its signs are distributed.")


def products(a):
    a("multiplying-monomials-polynomials/kp1", "Multiply $({a}x^3)(2x^2)$ as a single monomial.", "2*a*x**5", "Coefficients multiply to $2({a})$ and exponents add: $3+2=5$.", "Multiply coefficients and add exponents of the shared variable.")
    a("multiplying-monomials-polynomials/kp2", "Expand ${a}x(2x^2+x+3)$ in standard form.", "2*a*x**3+a*x**2+3*a*x", "Multiply ${a}x$ by each term: $2({a})x^3$, ${a}x^2$, and $3({a})x$.", "Distribute the outside monomial to every inside term.")
    a("multiplying-monomials-polynomials/kp3", "Expand $-{a}x(x^2-2x+3)$ in standard form.", "-a*x**3+2*a*x**2-3*a*x", "The three signed products are $-{a}x^3$, $2({a})x^2$, and $-3({a})x$.", "Determine each product's sign before combining terms.")
    a("multiplying-binomials/kp1", "Write the area of a rectangle with sides $x+{a}$ and $x+4$ as a standard-form polynomial.", "x**2+(a+4)*x+4*a", "The four products are $x^2$, $4x$, ${a}x$, and $4({a})$. Combine the two linear terms.", "Multiply each term of one side by each term of the other.")
    a("multiplying-binomials/kp2", "Expand $({a}x+1)(x+4)$ and combine like terms.", "a*x**2+(4*a+1)*x+4", "Products are ${a}x^2$, $4({a})x$, $x$, and $4$; add the two linear coefficients.", "Check both cross-products before combining them.")
    a("multiplying-binomials/kp3", "Expand $({a}x-1)(2x+3)$ in standard form.", "2*a*x**2+(3*a-2)*x-3", "The products are $2({a})x^2$, $3({a})x$, $-2x$, and $-3$.", "There are four term-pair products, including a negative constant product.")
    a("polynomial-multiplication/kp1", "Expand $(x+{a})(x^2+2x+3)$ by summing the six term products.", "x**3+(a+2)*x**2+(2*a+3)*x+3*a", "Multiplication by $x$ gives $x^3+2x^2+3x$; multiplication by ${a}$ gives ${a}x^2+2({a})x+3({a})$. Add like terms.", "Distribute each term of the binomial across the full trinomial.")
    a("polynomial-multiplication/kp2", "A box has dimensions $x$, $x+{a}$, and $x+3$. Write its volume in standard form.", "x**3+(a+3)*x**2+3*a*x", "First multiply the binomials to get $x^2+({a}+3)x+3({a})$, then multiply every term by $x$.", "Multiply two factors, simplify, then multiply the remaining factor.")
    a("polynomial-multiplication/kp3", "Expand $(x^2+{a}x+1)(x^2-x+2)$ and combine all like terms.", "x**4+(a-1)*x**3+(3-a)*x**2+(2*a-1)*x+2", "Group the nine products by power: cubic coefficients ${a}-1$, quadratic coefficients $2-{a}+1$, linear coefficients $2({a})-1$, and constant $2$.", "Use one column for each power from the fourth power down to the constant.", axes={"a":[2,4,5,6,7,8,9,10,11,12,13,14]})
    a("special-products/kp1", "Write the area of a square of side $x+{a}$ in standard form.", "x**2+2*a*x+a**2", "The square consists of $x^2$, two strips of area ${a}x$, and a corner of area $({a})^2$.", "A binomial square includes twice the product of its terms.", axes={'a': [7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18]})
    a("special-products/kp2", "Expand the conjugate product $(x+{a})(x-{a})$.", "x**2-a**2", "The cross-products $-{a}x$ and ${a}x$ cancel; the remaining products are $x^2$ and $-({a})^2$.", "Opposite middle products cancel in a conjugate pair.")
    a("special-products/kp3", "Expand $(2x+{a})^2$ using the special-product pattern.", "4*x**2+4*a*x+a**2", "The terms are $(2x)^2=4x^2$, $2(2x)({a})=4({a})x$, and $({a})^2$.", "Square the full first term and double the cross-product.", axes={'a': [6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]})
    a("binomial-cubes/kp1", "Expand $(x+{a})^3$ by squaring first, then multiplying by the remaining binomial.", "x**3+3*a*x**2+3*a**2*x+a**3", "The square is $x^2+2({a})x+({a})^2$. Multiply it by $x+{a}$; the quadratic and linear product groups each collect three equal contributions.", "Keep the intermediate square, then distribute the final factor.", axes={'a': [6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]})
    a("binomial-cubes/kp2", "Expand $(x-{a})^3$ with all signs shown correctly.", "x**3-3*a*x**2+3*a**2*x-a**3", "The square is $x^2-2({a})x+({a})^2$. Multiplication by $x-{a}$ gives negative quadratic, positive linear, and negative constant terms.", "Track the alternating signs of a cube of a difference.", axes={'a': [6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]})
    a("binomial-cubes/kp3", "Expand $(2x+{a})^3$ using the binomial-cube formula.", "8*x**3+12*a*x**2+6*a**2*x+a**3", "Substitute $2x$ and ${a}$ into the four terms: $(2x)^3$, $3(2x)^2({a})$, $3(2x)({a})^2$, and $({a})^3$.", "Use the four binomial coefficients in order while the first power decreases.")


def divisions(a):
    a("dividing-polynomials-by-monomials/kp1", r"Divide $({a}x^3+6x^2)/(2x)$ term by term, with $x\ne0$.", "a/2*x**2+3*x", "Divide the first coefficient ${a}$ by $2$, then subtract exponents; the second quotient is $6x^2/(2x)=3x$.", "Divide every numerator term by the full monomial divisor.", axes={"a":range(4,28,2)})
    a("dividing-polynomials-by-monomials/kp2", r"Divide $(-{a}x^3+6x^2)/(-x)$, with $x\ne0$.", "a*x**2-6*x", "A negative divided by a negative is positive; a positive divided by the negative divisor is negative. Subtract one from each exponent.", "Track the sign of each quotient separately.")
    a("dividing-polynomials-by-monomials/kp3", "Divide $({a}x^2y^3+6xy^2)/(xy)$ for nonzero $x,y$.", "a*x*y**2+6*y", "Subtract the divisor's exponents independently: $x^2y^3/(xy)=xy^2$ and $xy^2/(xy)=y$.", "Handle the exponent of each variable separately in each term.")
    a("polynomial-division/kp1", "Use long division on $(x^2+{a}x+{b})/(x+1)$ and give the quotient.", "x+a-1", "The first quotient term is $x$. Subtract $x^2+x$, leaving $({a}-1)x+{b}$; the next quotient term is ${a}-1$, with zero remainder.", "Divide leading terms, multiply back, and subtract before the next step.", derived={"b":"a-1"})
    a("polynomial-division/kp2", "Use long division on $(x^2+{a}x+{a})/(x+1)$. Report the ordered pair (quotient, remainder).", "(x+a-1,1)", "After quotient term $x$, the remaining numerator is $({a}-1)x+{a}$. Subtract $({a}-1)(x+1)$ to leave remainder $1$.", "The remainder must have lower degree than the divisor.")
    a("polynomial-division/kp3", "Use long division on $(x^3-{a}x+{b})/(x-1)$. Include the missing quadratic placeholder and give the quotient.", "x**2+x+1-a", "Use coefficients $1,0,-{a},{b}$. Successive quotient terms are $x^2$, $x$, and $1-{a}$; the final subtraction leaves zero.", "Write a zero coefficient for the missing quadratic term before dividing.", derived={"b":"a-1"})
    a("synthetic-division/kp1", "Use synthetic division on $x^2+{a}x+{b}$ by $x+1$. Give the quotient.", "x+a-1", "Use synthetic value $-1$ with row $1,{a},{b}$. Bring down $1$, add $-1$ to ${a}$, then multiply again; the last entry is zero.", "Use the zero of the divisor as the synthetic value.", derived={"b":"a-1"})
    a("synthetic-division/kp2", "Use synthetic division on $x^3+{a}x+2$ by $x-1$. Report (quotient, remainder) in that order.", "(x**2+x+a+1,a+3)", "The coefficient row is $1,0,{a},2$. Synthetic value $1$ produces quotient coefficients $1,1,{a}+1$ and final remainder ${a}+3$.", "Include the missing quadratic coefficient as zero before the synthetic steps.")
    a("synthetic-division/kp3", "Without dividing, find the remainder when $p(x)=x^3+{a}x+4$ is divided by $x-2$.", "12+2*a", "The remainder theorem gives $p(2)=2^3+{a}(2)+4=8+2({a})+4$.", "Evaluate the dividend at the zero of the linear divisor.")
