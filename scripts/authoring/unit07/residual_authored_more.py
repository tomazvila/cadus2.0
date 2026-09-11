"""Worked examples for shifted cube identities and mixed-response applications."""
SKETCHES = {
    "sum-difference-of-cubes/kp1": [
        "The cube roots are $x$ and $2$. Their difference is $x-2$; the second factor is $x^2+2x+4$. Multiplication cancels the two middle powers and leaves $x^3-8$.",
        "Use cube roots $x+1$ and $3$. Their difference is $x-2$. The quadratic $(x+1)^2+3(x+1)+9$ expands to $x^2+5x+13$, giving $(x-2)(x^2+5x+13)$.",
        "The coefficients give $x^3-64=x^3-4^3$. The difference-of-cubes identity gives $(x-4)(x^2+4x+16)$, with positive signs throughout the quadratic.",
        "The cube roots are $x+2$ and $5$. The quadratic must use a positive product term: $(x+2)^2+5(x+2)+25=x^2+9x+39$. The linear factor is $x-3$.",
    ],
    "sum-difference-of-cubes/kp2": [
        "The cube roots are $x$ and $1$. Their sum is $x+1$, while the quadratic uses the opposite middle sign: $x^2-x+1$.",
        "Use cube roots $x-1$ and $4$. Their sum is $x+3$. The quadratic $(x-1)^2-4(x-1)+16$ expands to $x^2-6x+21$.",
        "The coefficient table gives $x^3+8=x^3+2^3$. Form $(x+2)(x^2-2x+4)$: the negative middle term cancels the cross terms when the factors are multiplied.",
        "The cube roots are $x-2$ and $5$. Their sum is $x+3$, and the quadratic uses subtraction: $(x-2)^2-5(x-2)+25=x^2-9x+39$.",
    ],
    "sum-difference-of-cubes/kp3": [
        "The cube roots of the complete terms are $2x$ and $3$. Form $(2x-3)((2x)^2+(2x)(3)+3^2)=(2x-3)(4x^2+6x+9)$.",
        "The cube roots are $3x$ and $1$. Their sum is $3x+1$; the second factor is $(3x)^2-(3x)(1)+1=9x^2-3x+1$.",
        "The table reconstructs $8x^3-1=(2x)^3-1^3$. Use their difference $2x-1$ and the quadratic $4x^2+2x+1$.",
        "The cube roots are $4x$ and $3$. A sum uses $4x+3$ with quadratic $(4x)^2-(4x)(3)+3^2=16x^2-12x+9$. Correcting the middle sign cancels the cross terms.",
    ],
    "parabola-vertex-form/kp3": [
        "The square is centered at $x=2$, so that is the symmetry axis. Substituting $x=0$ gives $(0-2)^2-1=3$.",
        "At $x=1$, the inner quantity is $2$. Squaring gives $4$, and the leading negative sign gives $-4+4=0$ for the table entry.",
        "The inner expression $x-1$ vanishes at $x=1$, giving the axis. At the requested input $x=3$, the ordinate is $(3-1)^2+2=6$.",
        "The axis is $x=2$. Inputs $0$ and $4$ are equally distant from it, so both have ordinate $1$. Direct substitution confirms $-(4-2)^2+5=1$.",
    ],
    "quadratic-graphs-vertex/kp2": [
        "Factor $x^2-4x+3=(x-1)(x-3)$ for intercepts $(1,0)$ and $(3,0)$. Their midpoint is $x=2$; substitution gives $y=4-8+3=-1$, locating the vertex.",
        "The vertex input is $-2/(2\u00b71)=-1$. Evaluating gives $1-2-8=-9$, so the vertex is $(-1,-9)$. At zero input, $y=-8$, giving $(0,-8)$.",
        "The row gives $y=x^2-6x+8=(x-2)(x-4)$. Its zeros give $(2,0)$ and $(4,0)$. At their midpoint $x=3$, $y=9-18+8=-1$, so the vertex is $(3,-1)$.",
        "At $x=0$, only the constant remains: the intercept is $(0,-3)$. The vertex input is $2/2=1$, where $y=1-2-3=-4$; thus the vertex is $(1,-4)$.",
    ],
    "quadratic-applications/kp1": [
        "Let width be $w$ and length $w+3$. Then $w(w+3)=40$, so $(w+8)(w-5)=0$. The roots are $-8$ and $5$; a positive width requires $5$.",
        "Let the smaller positive integer be $n$. The equation $n(n+1)=56$ becomes $(n+8)(n-7)=0$. Discard $-8$ and use $n=7$; the next integer is $8$.",
        "Width $w$ satisfies $w(w+4)=45$, or $(w+9)(w-5)=0$. The negative root $-9$ cannot be a dimension, leaving width $5$.",
        "Let the number of rows be $n$, so there are $n+1$ columns. Then $n(n+1)=90$, or $(n-9)(n+10)=0$. Counts must be positive: $9$ rows and $10$ columns.",
    ],
    "quadratic-applications/kp2": [
        "Set height to zero and factor $-5t^2+10t=-5t(t-2)$. The roots are launch time $0$ and return time $2$; the requested positive landing time is $2$ seconds.",
        "The negative quadratic coefficient makes the vertex a maximum. Its time is $-30/(2(-5))=3$ seconds. The height there is $-5(3)^2+30(3)=45$ metres.",
        "At ground level, $-5t(t-4)=0$. The root $0$ describes launch, while the positive root $4$ describes the return to ground: $4$ seconds.",
        "The vertex time is $-40/(2(-5))=4$ seconds. Since the parabola opens downward, $h(4)=-5(4)^2+40(4)=80$ metres is the peak height.",
    ],
}
EDITS = {
    ("sum-difference-of-cubes/kp1", 1): {
        "problem": "Factor the shifted cube difference $(x+1)^3-27$ over the integers.",
        "answer": "(x-2)*(x^2+5*x+13)",
    },
    ("sum-difference-of-cubes/kp1", 3): {
        "problem": "A learner uses a negative product term in the quadratic factor of $(x+2)^3-125$. Give the corrected factorization with expanded factors.",
        "answer": "(x-3)*(x^2+9*x+39)",
    },
    ("sum-difference-of-cubes/kp2", 1): {
        "problem": "Factor the shifted cube sum $(x-1)^3+64$ over the integers.",
        "answer": "(x+3)*(x^2-6*x+21)",
    },
    ("sum-difference-of-cubes/kp2", 3): {
        "problem": "A learner uses a positive product term in the quadratic factor of $(x-2)^3+125$. Give the corrected factorization with expanded factors.",
        "answer": "(x+3)*(x^2-9*x+39)",
    },
}
