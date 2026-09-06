"""Bounded arithmetic families, retaining carrying, borrowing and word objectives."""
from common import condition as c, lit, op, recipe


def roots():
    rows = []
    for kp, limits in [(1, (0, 10)), (2, (11, 18))]:
        rows.append(recipe(f"single-digit-addition/kp{kp}", "Compute ${a} + {b}$.",
            "a+b", dict(a=range(10), b=range(10)), lambda a,b: a+b,
            "Count on from ${a}$ by ${b}$; when the sum crosses ten, fill ten first and add the remaining units.",
            "How many units can you add before reaching ten?", [
                c("ge", op("add", "a", "b"), lit(limits[0])),
                c("le", op("add", "a", "b"), lit(limits[1]))]))
    for kp, values in [(1, range(11)), (2, range(11,19))]:
        rules = [c("ge", "a", "b")]
        if kp == 2:
            rules.append(c("lt", op("sub", "a", "b"), lit(10)))
        rows.append(recipe(f"subtraction-facts/kp{kp}", "Compute ${a} - {b}$.",
            "a-b", dict(a=values,b=range(2,10)), lambda a,b:a-b,
            "Count back ${b}$ units from ${a}$. If you pass ten, subtract down to ten before taking away the remaining units.",
            "Where do you land when you count back the requested units?", rules))
    for kp, aa, bb in [(1,range(2,11),range(2,11)),(2,[11,12],range(2,13))]:
        rows.append(recipe(f"multiplication-tables/kp{kp}", r"Compute ${a} \times {b}$.",
            "a*b",dict(a=aa,b=bb),lambda a,b:a*b,
            "Count ${a}$ equal groups of ${b}$ by repeated addition.",
            "Which multiplication fact gives the total of these equal groups?"))
    rules = [c("divides","b","a"),c("ge","a",op("mul",lit(2),"b")),
             c("le","a",op("mul",lit(10),"b"))]
    for kp, text in [(1,r"Compute ${a} \div {b}$."),
                     (2,r"Solve: ${b} \times \square = {a}$.")]:
        rows.append(recipe(f"division-facts/kp{kp}",text,"a/b",
            dict(a=range(12,41),b=range(2,11)),lambda a,b:a//b,
            "Find the multiplication fact with factor ${b}$ and product ${a}$; its other factor is the quotient.",
            "What must multiply the known factor to make the product?",rules))
    return rows


def addition():
    rows = []
    specs = [
        ("addition-with-carrying/kp1",range(25,49),[26],"two-digit addition with a units carry"),
        ("addition-with-carrying/kp2",range(265,289),[176],"units and tens carries"),
        ("multi-digit-addition-subtraction/kp1",range(1865,1889),[3586],"multiple carries in four-digit addition"),
    ]
    for key, aa, bb, detail in specs:
        rules = [c("ge",op("add",op("mod","a",lit(10)),op("mod","b",lit(10))),lit(10))]
        rows.append(recipe(key,"Compute ${a} + {b}$.","a+b",dict(a=aa,b=bb),
            lambda a,b:a+b,
            f"Align place values for {detail}. Add each column from the units leftward; exchange each ten units in a column for one unit in the next column.",
            "Which columns need an exchange into the next place?",rules))
    rows.append(recipe("addition-with-carrying/kp3","Compute ${a} + {b} + 28$.",
        "a+b+28",dict(a=range(31,47),b=[19,27]),lambda a,b:a+b+28,
        "Align all three addends by place value. Add their units first, carry any full tens, then add the tens and the carried amount.",
        "What is the units-column total when you include all three addends?"))
    return rows


def subtraction():
    rows=[]
    specs=[("subtraction-with-borrowing/kp1",range(61,85),[27],"one borrow"),
           ("subtraction-with-borrowing/kp2",[n for n in range(431,457) if '0' not in str(n)],[187],"borrowing between nonzero places"),
           ("subtraction-with-borrowing/kp3",range(91,117),[38],"an addition check")]
    for key, aa, bb, detail in specs:
        rules=[c("lt",op("mod","a",lit(10)),op("mod","b",lit(10)))]
        rows.append(recipe(key,"Compute ${a} - {b}$ and check the difference by addition.",
            "a-b",dict(a=aa,b=bb),lambda a,b:a-b,
            f"Subtract by place value with {detail}. Exchange a higher-place unit when needed; then add the difference to ${{b}}$ to recover ${{a}}$.",
            "Which column needs an exchange, and how can addition check your difference?",rules))
    rows.append(recipe("multi-digit-addition-subtraction/kp2","Compute $2003 - {a}$.",
        "2003-a",dict(a=range(267,291)),lambda a:2003-a,
        "Exchange one thousand for hundreds, one of those hundreds for tens, and one ten for units. Subtract the units, tens and hundreds of ${a}$ from the regrouped places.",
        "How can you pass an exchange across the zero columns?"))
    rows.append(recipe("multi-digit-addition-subtraction/kp3","Compute ${a} + 286 - 173$.",
        "a+286-173",dict(a=range(361,377)),lambda a:a+286-173,
        "Add the first two quantities by place value; subtract the final quantity from that subtotal.",
        "What subtotal do you need before performing the subtraction?"))
    return rows


def words():
    specs=[
        ("addition-subtraction-word-problems/kp1","A museum had ${a}$ tickets and printed $286$ more. How many tickets does it have now?","a+286",range(1301,1317),lambda a:a+286,"Add the newly printed tickets to the original supply.","What operation combines the original supply and the new tickets?"),
        ("addition-subtraction-word-problems/kp2","A shop had ${a}$ notebooks, sold $86$, then received $47$ more. How many notebooks remain?","a-86+47",range(261,277),lambda a:a-86+47,"Subtract the sold notebooks from the initial stock, then add the delivery to the remaining stock.","Which change decreases the stock, and which change increases it?"),
        ("addition-subtraction-word-problems/kp3","Ava has ${a}$ cards, which is $57$ more than Leo has. How many cards does Leo have?","a-57",range(181,197),lambda a:a-57,"Ava's total includes Leo's total and the extra cards. Subtract the extra cards to find Leo's total.","How does the stated difference connect the two totals?"),
        ("multiplication-division-word-problems/kp1","A hall has ${a}$ rows with $17$ seats in each row. How many seats are there?","a*17",range(21,37),lambda a:a*17,"Multiply the number of equal rows by the seats in each row.","What multiplication counts every seat once?"),
        ("multiplication-division-word-problems/kp2","${a}$ pencils are packed with $7$ pencils in each box. How many boxes are filled?","a/7",range(301,407,7),lambda a:a//7,"Divide the total number of pencils by the number in one box. Multiply the quotient by the box size to check the total.","How many groups of the box size make the total?"),
        ("multiplication-division-word-problems/kp3","${a}$ pupils need transport. Each minibus has $18$ seats. How many minibuses are needed for everyone?","ceiling(a/18)",range(137,153),lambda a:(a+17)//18,"Divide pupils by seats per minibus. When pupils remain, another minibus is required even if it is partly empty.","Can any pupil left after filling the full minibuses travel without another vehicle?"),
        ("division-with-remainders/kp3","${a}$ oranges are packed in bags holding $7$ oranges each. How many full bags can be packed?","floor(a/7)",range(31,47),lambda a:a//7,"Count complete groups of the bag size. Set aside leftover oranges because they cannot fill another bag.","Does a partly filled bag count as a full bag?"),
    ]
    return [recipe(key,text,expr,dict(a=aa),fn,sketch,hint) for key,text,expr,aa,fn,sketch,hint in specs]
