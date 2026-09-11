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
    "slope/kp1": ({"kind":"required_form", "form":"reduced_fraction"}, [
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

COORD3 = {"kind": "coordinates", "arity": 3}
COORD4 = {"kind": "coordinates", "arity": 4}


def standard(points, direction=None):
    return dict(line(points, direction), type="standard")


AUTHORED.update({
    "proportional-relationships/kp1": (EXACT, [
        task("Use the proportional equation $y=(5/2)x$ to find $y$ at $x=6$.", "15",
             "The rule multiplies each input by $5/2$, giving $(5/2)6=15$.", {"type":"product","factors":["5/2",6]}),
        task("A proportional rule sends $3$ to $21$. Write its multiplier mentally, then find the output for $8$.", "56",
             "First solve $21=3k$ to get $k=7$. The same rule gives $y=7(8)=56$.", {"type":"proportional_predict","pair":[3,21],"input":8}),
        task("The equation $d=4t$ models distance in kilometres after $t$ hours. Give the numerical distance after half an hour.", "2",
             "Insert $t=1/2$: $d=4(1/2)=2$ kilometres.", {"type":"product","factors":[4,"1/2"]}),
        task("For $y=3x$, a learner adds $3$ to input $5$. Correct the output using the proportional equation.", "15",
             "The coefficient is a multiplier: $y=3(5)=15$; adding would use a different rule.", {"type":"product","factors":[3,5]}),
    ]),
    "graphing-proportional-relationships/kp2": (EXACT, [
        task("On a straight graph through the origin, a marked point is $5$ horizontal units and $20$ vertical units from the origin. Read $k$ for $y=kx$.", "4",
             "The marked point is $(5,20)$, so $5k=20$ and $k=4$.", {"type":"ratio","input":5,"output":20}),
        task("The $x$-axis uses $2$ units per square and the $y$-axis $3$ units per square. A proportional line passes through a mark $2$ squares right and $1$ square up. Find $k$.", "3/4",
             "Convert grid counts to coordinates: $x=2(2)=4$ and $y=3(1)=3$. Thus $k=3/4$.", {"type":"ratio","input":4,"output":3}),
        task("A proportional graph has axes input $x$ and output $y$. A marked point projects to $x=6$ and $y=9$. Determine the line's multiplier.", "3/2",
             "The projections give $(6,9)$. Dividing the vertical coordinate by the horizontal gives $9/6=3/2$.", {"type":"ratio","input":6,"output":9}),
        task("A learner reads $k=3$ from a proportional graph's point $(12,4)$ by reversing the axes. Give the corrected constant.", "1/3",
             "In $y=kx$, the vertical coordinate is the output: $4=12k$, so $k=1/3$.", {"type":"ratio","input":12,"output":4}),
    ]),
    "slope-from-a-graph/kp2": (EXACT, [
        task("A graph falls $4$ units as you move $2$ units right. Give its signed slope.", "-2",
             "Falling gives vertical change $-4$; rightward run is $2$, so the slope is $-4/2=-2$.", {"type":"ratio","input":2,"output":-4}),
        task("The right end of a graph segment is $6$ units lower than its left end and $8$ units farther right. Find the signed slope.", "-3/4",
             "Moving left to right changes height by $-6$ over a positive run of $8$, giving $-6/8=-3/4$.", {"type":"ratio","input":8,"output":-6}),
        task("Trace a descending line $5$ squares right and $3$ squares down on a unit grid. What signed rise per unit of run does the graph show?", "-3/5",
             "Downward motion makes the rise $-3$. Divide by the rightward run $5$ to keep the falling direction in the sign.", {"type":"ratio","input":5,"output":-3}),
        task("A learner assigns positive slope to a line descending $9$ units over a rightward run of $3$. Correct the signed slope.", "-3",
             "The magnitude is the quotient $9/3$; the descent makes the signed slope $-3$.", {"type":"ratio","input":3,"output":-9}),
    ]),
    "point-slope-form/kp1": (COORD3, [
        task("For slope $3$ through $(2,5)$, complete $y+A=M(x+B)$. Enter $(A,M,B)$, including signs.", "(-5,3,-2)",
             "Subtract the point's coordinates: $y-5=3(x-2)$, so the added offsets are $-5$ and $-2$.", {"type":"point_form","point":[2,5],"slope":3}),
        task("A line has slope $-1$ and contains $(-4,2)$. Fill $y+A=M(x+B)$ by giving $(A,M,B)$.", "(-2,-1,4)",
             "Using the point gives $y-2=-1(x-(-4))=-1(x+4)$.", {"type":"point_form","point":[-4,2],"slope":-1}),
        task("A line rises $1$ unit per $2$ right and contains $(3,-6)$. Use that point in $y+A=M(x+B)$; report $(A,M,B)$.", "(6,1/2,-3)",
             "The slope is $1/2$. Subtracting the vertical coordinate $-6$ produces $y+6$, while the horizontal offset is $x-3$.", {"type":"point_form","point":[3,-6],"slope":"1/2"}),
        task("A student used $y-4=-2(x-1)$ for slope $-2$ through $(-1,-4)$. Correct the offsets in $y+A=M(x+B)$ and give $(A,M,B)$.", "(4,-2,1)",
             "Subtracting negative point coordinates gives $y+4=-2(x+1)$; the slope remains $-2$.", {"type":"point_form","point":[-1,-4],"slope":-2}),
    ]),
    "point-slope-standard-form/kp1": (COORD3, [
        task("Convert $y=2x+3$ to $Ax+By=C$. Give integer $(A,B,C)$ with $A>0$ and no common factor.", "(2,-1,-3)",
             "Move $y$ left and the constant right: $2x-y=-3$. The coefficients already have greatest common divisor $1$.", standard([[0,3]], [1,2])),
        task("Rewrite $y-1=(1/2)(x-4)$ as $Ax+By=C$. Return primitive integer $(A,B,C)$ with positive $A$.", "(1,-2,2)",
             "Multiply by $2$: $2y-2=x-4$. Collect terms to get $x-2y=2$.", standard([[4,1]], [2,1])),
        task("Clear fractions in $y=-(3/4)x+2$ to get $Ax+By=C$. Enter integer $(A,B,C)$ with positive $A$ and greatest common divisor $1$.", "(3,4,8)",
             "Multiplication by $4$ gives $4y=-3x+8$. Move the $x$ term left to obtain $3x+4y=8$.", standard([[0,2]], [4,-3])),
        task("For $y+3=2(x-1)$, a student writes $2x-y=1$. Correct the primitive standard-form record $(A,B,C)$, keeping $A>0$.", "(2,-1,5)",
             "Expand to $y+3=2x-2$, hence $y=2x-5$ and $2x-y=5$.", standard([[1,-3]], [1,2])),
    ]),
    "point-slope-standard-form/kp2": (COORD, [
        task("Convert $3x+2y=12$ to slope-intercept form.", "(-3/2,6)",
             "Isolate $2y=12-3x$, then divide each term by $2$ to get slope $-3/2$ and intercept $6$.", line([[0,6],[4,0]])),
        task("For $4x-5y=20$, isolate $y$ to identify the complete slope-intercept rule.", "(4/5,-4)",
             "Rearrange to $-5y=20-4x$ and divide by $-5$, giving slope $4/5$ and intercept $-4$.", line([[0,-4],[5,0]])),
        task("Rewrite $-2x+3y=9$ in the form $y=mx+b$.", "(2/3,3)",
             "Add $2x$ to get $3y=2x+9$; division by $3$ gives the two coefficients.", line([[0,3],[3,5]])),
        task("A student converted $6x+2y=-8$ into $y=-3x+4$. Correct the slope-intercept rule.", "(-3,-4)",
             "After subtracting $6x$, divide $2y=-6x-8$ by $2$. The constant is $-4$.", line([[0,-4],[-2,2]])),
    ]),
    "point-slope-standard-form/kp3": (COORD3, [
        task("Find the line through $(1,2)$ and $(3,8)$ in $Ax+By=C$. Give primitive integer $(A,B,C)$ with $A>0$.", "(3,-1,1)",
             "Slope is $6/2=3$, so $y-2=3(x-1)$. Rearranging gives $3x-y=1$.", standard([[1,2],[3,8]])),
        task("The table rows $(2,5)$ and $(4,6)$ belong to one line. Give its primitive standard-form coefficients $(A,B,C)$ with positive $A$.", "(1,-2,-8)",
             "The rise is $1$ over run $2$. Clear the fraction in $y-5=(1/2)(x-2)$ to obtain $x-2y=-8$.", standard([[2,5],[4,6]])),
        task("A line joins $(-2,5)$ and $(1,-1)$. Write $Ax+By=C$ using integer coefficients with no common factor and $A>0$; enter $(A,B,C)$.", "(2,1,1)",
             "The slope is $-6/3=-2$. From $y+1=-2(x-1)$, collect terms to obtain $2x+y=1$.", standard([[-2,5],[1,-1]])),
        task("The line through $(-3,-2)$ and $(3,2)$ was recorded as $4x-6y=0$. Reduce its standard-form coefficients to primitive integer $(A,B,C)$ with $A>0$.", "(2,-3,0)",
             "The slope is $4/6=2/3$ and the line passes through the origin. Divide the proposed coefficients by $2$ to get $2x-3y=0$.", standard([[-3,-2],[3,2]])),
    ]),
    "parallel-perpendicular-lines/kp3": (COORD, [
        task("Find a line through the origin perpendicular to $2x+3y=6$.", "(3/2,0)",
             "The reference slope is $-2/3$, so a perpendicular has slope $3/2$. Through the origin means intercept $0$.", {"type":"perpendicular","points":[[0,0]],"reference":[3,-2]}),
        task("A line perpendicular to $3x-2y=4$ must pass through $(3,1)$. Give its slope-intercept rule.", "(-2/3,3)",
             "The reference has slope $3/2$, so use $-2/3$. Then $1=(-2/3)3+b$ gives $b=3$.", {"type":"perpendicular","points":[[3,1]],"reference":[2,3]}),
        task("Find the slope-intercept rule parallel to $4x+2y=8$ through $(-1,5)$.", "(-2,3)",
             "The reference equation gives slope $-2$. Keeping it and inserting $(-1,5)$ gives $5=2+b$.", line([[-1,5]], [1,-2])),
        task("A student takes slope $-5$ for a perpendicular to $x+5y=10$ through $(1,2)$. Correct the complete slope-intercept rule.", "(5,-3)",
             "The reference slope is $-1/5$, whose negative reciprocal is $5$. The point gives $2=5+b$, so $b=-3$.", {"type":"perpendicular","points":[[1,2]],"reference":[5,-1]}),
    ]),
    "linear-word-problems/kp2": (COORD, [
        task("A candle is $20$ cm tall after $2$ hours and $14$ cm after $5$ hours. For height $h=mt+b$, enter $(m,b)$ with $m$ in cm/hour and $b$ in cm.", "(-2,24)",
             "The height falls $6$ cm over $3$ hours, so $m=-2$. The first measurement gives $20=-2(2)+b$, hence $b=24$.", {"type":"model","points":[[2,20],[5,14]]}),
        task("A gym charges a fixed joining fee plus a monthly rate. Total cost is EUR $95$ for $3$ months and EUR $215$ for $7$ months. In $C=mt+b$, return $(m,b)$ in EUR/month and EUR.", "(30,5)",
             "Four extra months cost $215-95=120$, giving $m=30$. Remove three monthly payments from $95$ to get the fee $5$.", {"type":"model","points":[[3,95],[7,215]]}),
        task("A tank fills at a constant rate. It contains $17$ litres after $2$ minutes and $32$ litres after $5$ minutes. For $V=mt+b$, report $(m,b)$ in litres/minute and litres.", "(5,7)",
             "Volume increases by $15$ litres over $3$ minutes. The rate is $5$ and the initial volume is $17-5(2)=7$.", {"type":"model","points":[[2,17],[5,32]]}),
        task("A taxi's fare is a fixed fee plus cost per kilometre. Trips of $4$ km and $9$ km cost EUR $14$ and EUR $29$. Recover $(m,b)$ in $C=md+b$, in EUR/km and EUR.", "(3,2)",
             "The extra $5$ km costs $15$, so the rate is $3$. Subtract $3(4)$ from $14$ to obtain the fixed fee $2$.", {"type":"model","points":[[4,14],[9,29]]}),
    ]),
    "graphing-linear-equations/kp2": (COORD4, [
        task("To draw $2x+3y=6$ using intercepts, find the horizontal-axis point first and vertical-axis point second. Enter $(x_1,y_1,x_2,y_2)$.", "(3,0,0,2)",
             "At $y=0$, $2x=6$ gives $(3,0)$. At $x=0$, $3y=6$ gives $(0,2)$. Plot and join those two points.", {"type":"intercepts","abc":[2,3,6]}),
        task("For a graph of $x-y=4$, give the two axis crossings as $(x_1,y_1,x_2,y_2)$, horizontal-axis crossing first.", "(4,0,0,-4)",
             "Setting $y=0$ gives $x=4$; setting $x=0$ gives $y=-4$. Join $(4,0)$ and $(0,-4)$.", {"type":"intercepts","abc":[1,-1,4]}),
        task("Choose the intercept points for graphing $-3x+2y=12$. Report $(x_1,y_1,x_2,y_2)$ with the point on the horizontal axis first.", "(-4,0,0,6)",
             "The horizontal crossing solves $-3x=12$ and is $(-4,0)$. The vertical crossing solves $2y=12$ and is $(0,6)$.", {"type":"intercepts","abc":[-3,2,12]}),
        task("A student plotted $(2,0)$ and $(0,3)$ for $3x+2y=12$. Correct both intercepts and enter $(x_1,y_1,x_2,y_2)$, horizontal-axis point first.", "(4,0,0,6)",
             "Divide $12$ by the coefficient of the coordinate that remains nonzero: $x=12/3=4$ and $y=12/2=6$. Join the corrected points.", {"type":"intercepts","abc":[3,2,12]}),
    ]),
})

AUTHORED.update({
    "constant-of-proportionality/kp2": (EXACT, [
        task("A proportional table has rows $(x,y)=(2,5),(4,10),(6,15)$. Find the common multiplier $k$.", "5/2",
             "Each output divided by its input gives the same ratio: $5/2$, $10/4$, and $15/6$ all reduce to $5/2$.", {"type":"table_ratio","rows":[[2,5],[4,10],[6,15]]}),
        task("A table lists inputs $3,5$ above outputs $12,20$, respectively. Determine $k$ in $y=kx$.", "4",
             "The paired entries give $12/3$ and $20/5$. Both quotients are $4$, so the same multiplier fits both columns.", {"type":"table_ratio","rows":[[3,12],[5,20]]}),
        task("A proportional table includes $(0,0),(4,1),(12,3)$. Determine its constant without dividing by zero.", "1/4",
             "The origin supplies no ratio. Use a nonzero input: $1/4$ agrees with the other usable ratio $3/12$.", {"type":"table_ratio","rows":[[0,0],[4,1],[12,3]]}),
        task("For table rows $(2,3),(6,9)$, a learner reports $k=2/3$. Correct the multiplier from input $x$ to output $y$.", "3/2",
             "Use output over input: $3/2$ and $9/6$ agree. The learner reversed both ratios.", {"type":"table_ratio","rows":[[2,3],[6,9]]}),
    ]),
    "slope/kp3": (EXACT, [
        task("Find $c$ so that $(0,1),(2,5),(5,c)$ are collinear.", "11",
             "The first two points rise $4$ units over a run of $2$, giving slope $2$. From $x=2$ to $x=5$, rise must be $2(3)=6$, giving $c=5+6=11$.", {"type":"collinear","points":[[0,1],[2,5],[5,None]]}),
        task("The points $(-2,7),(1,1),(4,h)$ must lie on one line. Determine $h$.", "-5",
             "Equal horizontal gaps of $3$ require equal vertical changes. The first change is $1-7=-6$, so the next height is $1-6=-5$.", {"type":"collinear","points":[[-2,7],[1,1],[4,None]]}),
        task("Find the missing horizontal coordinate $u$ so that $(1,2),(3,8),(u,14)$ lie on one straight line.", "5",
             "The first rise $6$ over run $2$ gives slope $3$. Another rise of $6$ needs another run of $2$, making $u=3+2=5$.", {"type":"collinear","points":[[1,2],[3,8],[None,14]]}),
        task("A learner places $(6,8)$ on the line through $(-2,-1)$ and $(2,1)$. Correct only its height so the three points are collinear.", "3",
             "The known slope is $2/4=1/2$. Moving $4$ right from $(2,1)$ raises the line by $2$, so the corrected height is $3$.", {"type":"collinear","points":[[-2,-1],[2,1],[6,None]]}),
    ]),
})

AUTHORED["reading-slope-intercept-equations/kp3"] = (COORD, [
    task("For $y=(2/3)x-1$, give the smallest integer rise and positive integer run as $(r,s)$.", "(2,3)",
         "The coefficient of $x$ is already reduced. Moving $3$ right requires moving $2$ up.", {"type":"rise_run","slope":"2/3"}),
    task("The equation $y=-(4/6)x+5$ describes a line. Reduce its slope to a signed rise and smallest positive integer run; enter $(r,s)$.", "(-2,3)",
         "Reduce $-4/6$ by $2$ to $-2/3$. With a positive run of $3$, the rise is $-2$, indicating descent.", {"type":"rise_run","slope":"-2/3"}),
    task("In $y=x/4+2$, the numerator of the slope is implicit. Report the simplest integer rise/run pair $(r,s)$ with $s>0$.", "(1,4)",
         "The coefficient of $x$ is $1/4$, so the line rises one unit for each four-unit rightward run.", {"type":"rise_run","slope":"1/4"}),
    task("For $y=-(3/2)x-1$, a learner reports rise $3$, run $-2$. Give the simplest pair $(r,s)$ describing a rightward move, so $s>0$.", "(-3,2)",
         "Reverse both directions to make the run positive. Moving $2$ right then gives a signed rise of $-3$.", {"type":"rise_run","slope":"-3/2"}),
])
