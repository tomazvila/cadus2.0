"""Place value, multiplication, division, grouping and estimation recipes."""
from common import condition as c, lit, op, recipe


def place():
    specs=[
        ("place-value/kp1","What is the value of the tens digit in ${a}$?","10*(floor(a/10)-10*floor(a/100))",range(231,247),lambda a:(a//10%10)*10,"Read the tens digit and multiply its face value by ten.","Which digit occupies the tens column?"),
        ("place-value/kp2","What is the value of the thousands digit in ${a}$?","1000*(floor(a/1000)-10*floor(a/10000))",range(12001,27002,1000),lambda a:(a//1000%10)*1000,"Locate the thousands digit, including when other places hold zero, and multiply that digit by one thousand.","What is the place value of the fourth digit from the right?"),
        ("rounding-whole-numbers/kp1","Round ${a}$ to the nearest ten.","10*floor((a+5)/10)",range(61,81),lambda a:10*((a+5)//10),"Look at the units digit. Keep the tens digit for units below five; otherwise increase the tens digit. Replace the units digit with zero.","Does the units digit put the number below or at least halfway to the next ten?"),
        ("rounding-whole-numbers/kp2","Round ${a}$ to the nearest thousand.","1000*floor((a+500)/1000)",range(2491,2511),lambda a:1000*((a+500)//1000),"Locate the two neighboring thousands. Compare the number with their midpoint; at the midpoint round to the higher thousand.","Is the number below, at, or above the halfway point?"),
        ("multiplying-by-powers-of-ten/kp1",r"Compute ${a} \times 100$.","a*100",range(21,37),lambda a:a*100,"Multiplication by one hundred moves each digit two place-value columns to the left. Fill empty positions with zero.","Which place will each digit occupy after multiplying by one hundred?"),
        ("multiplying-by-one-digit/kp1",r"Compute ${a} \times 7$.","a*7",range(41,57),lambda a:a*7,"Multiply the units by the single-digit factor, carry the tens, then multiply the tens and add the carried amount.","How do the units product and tens product combine?"),
        ("multiplying-by-one-digit/kp2",r"Compute ${a} \times 6$.","a*6",range(301,317),lambda a:a*6,"Work from units to hundreds, multiplying each place by the one-digit factor and carrying into the next place.","How will you keep the internal zero's column in the calculation?"),
        ("multi-digit-multiplication/kp1",r"Compute ${a} \times 36$.","a*36",range(41,57),lambda a:a*36,"Find the partial products for the units and tens of the second factor. Align their place values and add.","What partial products come from splitting the second factor into tens and units?"),
        ("multi-digit-multiplication/kp2",r"Compute ${a} \times 24$.","a*24",range(211,227),lambda a:a*24,"Multiply the three-digit factor by each place of the two-digit factor; shift the tens partial product one place before adding.","Which partial product represents multiplication by tens?"),
        ("multi-digit-multiplication/kp3",r"Compute ${a} \times 43$ using partial products.","a*43",range(61,77),lambda a:a*43,"Split the second factor into forty and three. Add the product with forty to the product with three, then compare with a rounded estimate.","How can the distributive property break this into two easier products?"),
    ]
    return [recipe(key,text,expr,dict(a=aa),fn,sketch,hint) for key,text,expr,aa,fn,sketch,hint in specs]


def multiplication():
    rows=[recipe("multiplying-by-powers-of-ten/kp2",r"Compute ${a} \times {b}$.",
        "a*b",dict(a=range(20,91,10),b=[20,30,40]),lambda a,b:a*b,
        "Multiply the nonzero tens digits using the multiplication table. Each factor contributes a factor of ten, so scale that product by one hundred.",
        "What table fact remains after factoring out a ten from each operand?")]
    rows.append(recipe("multiplying-by-one-digit/kp3",r"Compute ${a} \times {b}$ mentally.",
        "a*b",dict(a=[29,39,49,59,69,79],b=[3,4,6]),lambda a,b:a*b,
        "Replace the first factor by the next multiple of ten, multiply, then subtract one group of the second factor.",
        "How does increasing the first factor by one change the product?"))
    return rows


def division():
    specs=[("long-division-one-digit/kp1",range(44,97,4),4),
           ("long-division-one-digit/kp2",range(203,316,7),7),
           ("long-division/kp1",range(2304,2433,8),8),
           ("long-division/kp2",range(312,505,12),12)]
    return [recipe(key,rf"Compute ${{a}} \div {divisor}$.",f"a/{divisor}",
        dict(a=aa),lambda a,d=divisor:a//d,
        "Divide from the highest place, multiply the quotient digit by the divisor, subtract, and bring down the next digit. Check by multiplying the quotient by the divisor.",
        "After subtracting each partial product, which digit should you bring down next?")
        for key,aa,divisor in specs]


def grouping():
    specs=[
        ("whole-number-exponents/kp2","Compute ${a}^{{2}}$.","a**2",dict(a=range(4,21)),lambda a:a*a,"A square is the product of two equal factors. Multiply the base by itself.","How many equal factors does a square contain?"),
        ("whole-number-exponents/kp3","Which value is larger: ${a}^{{2}}$ or ${b}^{{3}}$? Give its numerical value.","max(a**2,b**3)",dict(a=range(7,14),b=[3,4,5]),lambda a,b:max(a*a,b**3),"Compute the square and cube by repeated multiplication, then compare their numerical values.","What value does each power represent?"),
        ("expressions-with-parentheses/kp1",r"Compute $({a} + {b}) \times 4$.","(a+b)*4",dict(a=range(2,9),b=[3,6,8]),lambda a,b:(a+b)*4,"Add the two numbers inside the parentheses first, then multiply that subtotal by the outside factor.","Which sum is grouped before the multiplication?"),
        ("expressions-with-parentheses/kp2","Compute ${a} - (7 - 3)$.","a-(7-3)",dict(a=range(15,31)),lambda a:a-4,"Subtract inside the parentheses to find the amount being removed. Subtract that entire amount from the leading number.","How much does the grouped subtraction tell you to remove?"),
        ("expressions-with-parentheses/kp3","Evaluate: multiply the sum of ${a}$ and $6$ by $5$.","(a+6)*5",dict(a=range(3,19)),lambda a:(a+6)*5,"The phrase 'the sum' groups the addition. Find that sum before multiplying it by the final factor.","Which words tell you to group an addition?"),
        ("order-of-operations/kp1",r"Compute ${a} + {b} \times 6$.","a+b*6",dict(a=range(2,9),b=[3,5,7]),lambda a,b:a+b*6,"Find the product first because multiplication takes precedence over addition; then add the leading number.","Which operation must be completed before adding?"),
        ("order-of-operations/kp2",r"Compute ${a} + {b}^{{2}} \times 3$.","a+b**2*3",dict(a=range(2,9),b=[2,4,5]),lambda a,b:a+b*b*3,"Square the base, multiply that square by the following factor, then add the leading number.","What must you evaluate before forming the product?"),
        ("order-of-operations/kp3",r"Compute $({a} + 3)^{{2}} - {b} \times 2$.","(a+3)**2-b*2",dict(a=range(3,10),b=[2,4,6]),lambda a,b:(a+3)**2-b*2,"Evaluate the grouped sum and square it. Compute the separate product, then subtract that product from the square.","What two values must be ready before the final subtraction?"),
    ]
    return [recipe(key,text,expr,domains,fn,sketch,hint) for key,text,expr,domains,fn,sketch,hint in specs]


def estimation():
    return [
        recipe("rounding-estimation/kp1","Estimate ${a} + 36$ by rounding each number to the nearest ten.",
            "10*floor((a+5)/10)+40",dict(a=range(71,91)),lambda a:10*((a+5)//10)+40,
            "Round each addend independently to its nearest ten using the units digit. Add the rounded numbers to obtain the estimate.",
            "Which neighboring ten is closest to each addend?"),
        recipe("rounding-estimation/kp2",r"Estimate ${a} \times 28$ by rounding each factor to the nearest ten.",
            "10*floor((a+5)/10)*30",dict(a=range(41,61)),lambda a:10*((a+5)//10)*30,
            "Round each factor to the nearest ten, then multiply the rounded factors using their tens digits and place values.",
            "How can the rounded factors make multiplication easier?"),
        recipe("rounding-estimation/kp3","Without calculating exactly, is ${a} + 298$ closer to $600$ or $700$?",
            "100*floor((a+298+50)/100)",dict(a=range(331,371)),lambda a:100*((a+348)//100),
            "The midpoint of the two offered totals is six hundred fifty. Compare the sum with that midpoint to select the closer offered total.",
            "On which side of the midpoint between the two choices does the sum lie?",
            [c("ne","a",lit(352))]),
    ]
