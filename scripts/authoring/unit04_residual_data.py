"""Independent authored tasks and mathematical premises for Unit04's first batch."""
COORD = {"kind": "coordinates", "arity": 2}
EXACT = {"kind": "exact"}
LINE = COORD  # Ordered coefficients complete the scaffold y=mx+b.


def point(x, y):
    return [x, y]


def line(points, direction=None):
    result = {"type": "line", "points": points}
    if direction is not None:
        result["direction"] = direction
    return result


def task(problem, answer, sketch, premise):
    if premise["type"] in {"line", "perpendicular"}:
        problem += " Enter the coefficients as $(m,b)$ for $y=mx+b$."
    return dict(problem=problem, answer=answer, solution_sketch=sketch, premise=premise)


AUTHORED = {
    "plotting-points/kp1": (COORD, [
        task("Starting at the origin, move $3$ right and $5$ up. Give the final ordered pair.",
             "(3,5)", "Horizontal movement gives $x=3$; vertical movement gives $y=5$. Write horizontal first.",
             {"type": "position", "moves": [[3,0],[0,5]]}),
        task("A marker is at $(2,4)$. Move it $3$ units upward. Give its new coordinates.",
             "(2,7)", "The upward move preserves $x=2$ and adds $3$ to the old height: $4+3=7$.",
             {"type": "position", "moves": [[2,4],[0,3]]}),
        task("On a grid, column numbers increase rightward and row numbers upward from zero. Give the ordered pair at column $6$, row $1$.",
             "(6,1)", "Column is the horizontal coordinate and row is the vertical coordinate, so record $6$ before $1$.",
             {"type": "position", "moves": [[6,1]]}),
        task("A robot leaves $(0,0)$, goes $2$ right, $4$ up, then $3$ more right. Give its final coordinates.",
             "(5,4)", "Combine the two horizontal moves, $2+3=5$. The only vertical move is $4$ up.",
             {"type": "position", "moves": [[2,0],[0,4],[3,0]]}),
    ]),
    "constant-of-proportionality/kp1": (EXACT, [
        task("For $y=kx$, the pair $(4,10)$ is a solution. Find $k$.", "5/2",
             "The equation gives $10=4k$; dividing both sides by $4$ gives $k=5/2$.",
             {"type": "ratio", "input": 4, "output": 10}),
        task("A proportional relationship has output $18$ at input $6$. What number multiplies every input to obtain its output?", "3",
             "One input unit corresponds to $18/6=3$ output units, the same multiplier for all pairs.",
             {"type": "ratio", "input": 6, "output": 18}),
        task("A line through the origin passes through $(8,2)$. Determine its constant of proportionality.", "1/4",
             "For a line through the origin, $y=kx$. Thus $2=8k$ and $k=1/4$.",
             {"type": "ratio", "input": 8, "output": 2}),
        task("The proportional rule $y=2x$ is proposed for a relationship containing $(5,8)$. Replace the multiplier $2$ with the correct constant.", "8/5",
             "The proposed rule predicts $10$, while the given output is $8$. The corrected multiplier satisfies $5k=8$.",
             {"type": "ratio", "input": 5, "output": 8}),
    ]),
    "slope-from-a-graph/kp1": (EXACT, [
        task("On a line graph, a slope triangle goes $4$ squares right and $6$ squares up. Each square is one unit on both axes. Find the slope.", "3/2",
             "The vertical change is $6$ and horizontal change is $4$; rise over run is $6/4=3/2$.",
             {"type": "ratio", "input": 4, "output": 6}),
        task("Two marks on a rising graph are separated by $8$ horizontal grid intervals and $2$ vertical intervals. Both axes use one unit per interval. Find the slope.", "1/4",
             "The rise is $2$ units for a run of $8$, so the slope is $2/8=1/4$.",
             {"type": "ratio", "input": 8, "output": 2}),
        task("A graph's slope triangle has a horizontal leg of $3$ units and an upward vertical leg of $9$ units. How much does the line rise per horizontal unit?", "3",
             "Divide the $9$-unit rise among the $3$ horizontal units to obtain $3$ units of rise per unit of run.",
             {"type": "ratio", "input": 3, "output": 9}),
        task("A learner uses run over rise for a graph that rises $5$ units while running $2$ units right. Give the correct slope.", "5/2",
             "Slope measures vertical change per horizontal change, so the rise $5$ belongs in the numerator and run $2$ in the denominator.",
             {"type": "ratio", "input": 2, "output": 5}),
    ]),
    "slope/kp1": (EXACT, [
        task("Find the slope through $(1,5)$ and $(5,2)$.", "-3/4",
             "The vertical change is $2-5=-3$ and the horizontal change is $5-1=4$, giving $-3/4$.",
             {"type": "slope", "points": [[1,5],[5,2]]}),
        task("A line falls $6$ units for a rightward run of $9$ units. Give its slope as a reduced fraction.", "-2/3",
             "Falling makes the rise negative: $-6/9$ reduces by $3$ to $-2/3$.",
             {"type": "ratio", "input": 9, "output": -6}),
        task("A linear table contains the rows $(x,y)=(-2,-3)$ and $(4,0)$. Find the slope.", "1/2",
             "Subtract consistently: the output increases by $3$ while the input increases by $6$, so the slope is $3/6=1/2$.",
             {"type": "slope", "points": [[-2,-3],[4,0]]}),
        task("A slope calculation gives $(7-1)/(-5-(-1))$. Reduce the result and keep its sign.", "-3/2",
             "The numerator is $6$ and denominator is $-4$. Reducing $6/(-4)$ gives $-3/2$.",
             {"type": "slope", "points": [[-1,1],[-5,7]]}),
    ]),
    "reading-slope-intercept-equations/kp1": (COORD, [
        task("For $y=3x-5$, give $(m,b)$ in $y=mx+b$.", "(3,-5)",
             "The coefficient of $x$ is $3$; the signed constant is $-5$.",
             {"type": "coefficients", "m": 3, "b": -5}),
        task("A line is written $y=-2x+7$. Report its slope and vertical-axis intercept value, in that order as $(m,b)$.", "(-2,7)",
             "The multiplier of $x$ gives the slope $-2$. At $x=0$, the remaining value is $7$.",
             {"type": "coefficients", "m": -2, "b": 7}),
        task("The model $y=(1/2)x-3$ describes a line. Complete the coefficient record $(m,b)$.", "(1/2,-3)",
             "One additional unit of $x$ adds $1/2$ to $y$; the constant term places the intercept at $-3$.",
             {"type": "coefficients", "m": "1/2", "b": -3}),
        task("For $y=-4x-6$, a student records $(m,b)=(4,6)$. Give the corrected pair.", "(-4,-6)",
             "Both signs belong to the coefficients: the multiplier is $-4$ and the constant is $-6$.",
             {"type": "coefficients", "m": -4, "b": -6}),
    ]),
    "slope-intercept-form/kp1": (LINE, [
        task("Write $y=mx+b$ for a line with slope $4$ through $(2,7)$.", "(4,-1)",
             "Insert the point into $7=4(2)+b$, so $b=-1$. The slope stays $4$.", line([[2,7]], [1,4])),
        task("A line rises $1$ unit per $2$ units right and contains $(4,1)$. Write its slope-intercept equation.", "(1/2,-1)",
             "The slope is $1/2$. Substituting the point gives $1=(1/2)4+b$, hence $b=-1$.", line([[4,1]], [2,1])),
        task("The line $y=-3x+b$ contains $(-2,5)$. Determine $b$ and give the complete equation.", "(-3,-1)",
             "At the given point, $5=-3(-2)+b=6+b$, which gives $b=-1$.", line([[-2,5]], [1,-3])),
        task("A student draws $y=2x+1$ for a slope-$2$ line through $(3,4)$. Correct the equation while preserving the slope.", "(2,-2)",
             "The correct constant satisfies $4=2(3)+b$, so $b=-2$ instead of $1$.", line([[3,4]], [1,2])),
    ]),
    "slope-intercept-form/kp2": (LINE, [
        task("Write the slope-intercept equation through $(1,3)$ and $(4,9)$.", "(2,1)",
             "Slope is $(9-3)/(4-1)=2$. Then $3=2(1)+b$ gives $b=1$.", line([[1,3],[4,9]])),
        task("A straight-line table contains $(x,y)=(-2,5)$ and $(1,-1)$. Give its rule in slope-intercept form.", "(-2,1)",
             "The rate is $(-1-5)/(1-(-2))=-2$. Using $(-2,5)$ gives $5=4+b$ and $b=1$.", line([[-2,5],[1,-1]])),
        task("A segment joins $(-4,-1)$ to $(2,2)$. Find the slope-intercept equation of its supporting line.", "(1/2,1)",
             "The rise is $3$ over a run of $6$, so $m=1/2$. At $(2,2)$, $2=1+b$, giving $b=1$.", line([[-4,-1],[2,2]])),
        task("The proposed rule $y=3x$ must fit both $(2,2)$ and $(5,11)$. Replace it with the slope-intercept equation fitting both points.", "(3,-4)",
             "The two points give slope $(11-2)/(5-2)=3$. The first point then gives $2=6+b$, so $b=-4$.", line([[2,2],[5,11]])),
    ]),
    "slope-intercept-form/kp3": (LINE, [
        task("A line has x-intercept $(3,0)$ and y-intercept $(0,6)$. Write its slope-intercept equation.", "(-2,6)",
             "The y-intercept fixes $b=6$. Reaching $(3,0)$ requires slope $(0-6)/(3-0)=-2$.", line([[3,0],[0,6]])),
        task("A line crosses the horizontal axis at $-4$ and the vertical axis at $2$. Give $y=mx+b$.", "(1/2,2)",
             "Use $(-4,0)$ and $(0,2)$: the slope is $2/4=1/2$ and the vertical intercept is $2$.", line([[-4,0],[0,2]])),
        task("The graph meets the axes at $(5,0)$ and $(0,-5)$. Recover its slope-intercept rule.", "(1,-5)",
             "From $(0,-5)$ to $(5,0)$, rise and run are both $5$. Thus $m=1$ and $b=-5$.", line([[5,0],[0,-5]])),
        task("A student uses $y=-3x+6$ for intercepts $x=6$ and $y=3$. Correct the slope-intercept equation.", "(-1/2,3)",
             "The intercepts are $(6,0)$ and $(0,3)$, so $b=3$ and $m=(0-3)/6=-1/2$.", line([[6,0],[0,3]])),
    ]),
    "point-slope-form/kp3": (LINE, [
        task("Convert $y-3=2(x-2)$ to slope-intercept form.", "(2,-1)",
             "Distribute to obtain $y-3=2x-4$, then add $3$ to isolate $y$.", line([[2,3]], [1,2])),
        task("Rewrite $y+2=-3(x+1)$ as $y=mx+b$.", "(-3,-5)",
             "Expansion gives $y+2=-3x-3$. Subtracting $2$ gives the constant $-5$.", line([[-1,-2]], [1,-3])),
        task("Isolate $y$ and collect the constant in $y-4=(1/2)(x+6)$.", "(1/2,7)",
             "The product contributes $(1/2)x+3$; adding $4$ to both sides makes the constant $7$.", line([[-6,4]], [2,1])),
        task("A conversion of $y+5=-2(x-3)$ produced $y=-2x-11$. Give the corrected slope-intercept equation.", "(-2,1)",
             "The product $-2(x-3)$ is $-2x+6$. Subtract $5$, leaving constant $1$.", line([[3,-5]], [1,-2])),
    ]),
    "parallel-perpendicular-lines/kp2": (LINE, [
        task("Write the slope-intercept equation perpendicular to $y=2x+5$ through $(0,3)$.", "(-1/2,3)",
             "The perpendicular slope is $-1/2$. The point on the vertical axis directly gives $b=3$.",
             {"type": "perpendicular", "points": [[0,3]], "reference": [1,2]}),
        task("A line of slope $-3$ is perpendicular to a second line through $(3,-1)$. Find the second line's slope-intercept equation.", "(1/3,-2)",
             "The slopes must multiply to $-1$, so the new slope is $1/3$. Then $-1=(1/3)3+b$ gives $b=-2$.",
             {"type": "perpendicular", "points": [[3,-1]], "reference": [1,-3]}),
        task("The line through $(0,1)$ and $(2,2)$ has a perpendicular through $(-1,4)$. Write the perpendicular in slope-intercept form.", "(-2,2)",
             "The reference slope is $1/2$, so the new slope is $-2$. The given point gives $4=2+b$, hence $b=2$.",
             {"type": "perpendicular", "points": [[-1,4]], "reference": [2,1]}),
        task("A student chooses slope $4$ for a line perpendicular to $y=-4x+2$ through $(4,3)$. Correct the slope and give the full slope-intercept equation.", "(1/4,2)",
             "The negative reciprocal of $-4$ is $1/4$. Substitution gives $3=(1/4)4+b$ and $b=2$.",
             {"type": "perpendicular", "points": [[4,3]], "reference": [1,-4]}),
    ]),
}
