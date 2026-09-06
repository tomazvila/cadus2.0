"""GCF, LCM and ordered comparison recipes with explicitly bounded domains."""
import math
from common import condition as c, lit, op, recipe


def comparisons():
    return [
        recipe("comparing-ordering-whole-numbers/kp1","Which is larger, ${a}$ or ${b}$?",
            "max(a,b)",dict(a=range(4638,4654),b=[4646,4657]),lambda a,b:max(a,b),
            "Compare digit counts, then compare matching places from left to right. The first unequal digit determines the larger number.",
            "At which place do the two numbers first differ?",[c("ne","a","b")]),
        recipe("comparing-ordering-whole-numbers/kp2","Order from least to greatest: ${a}$, $427$, $312$.",
            "(min(a,312),max(312,min(a,427)),max(a,427))",dict(a=range(301,317)),
            lambda a:", ".join(map(str,sorted([a,427,312]))),
            "Compare the hundreds digits first and then tens and units when tied. Place the smallest first and the largest last.",
            "Which numbers share a hundreds digit and need a closer comparison?",[c("ne","a",lit(312))]),
    ]


def common_factors():
    rows=[]
    for kp, aa, bb in [(1,range(12,28),[18,24]),(2,range(32,48),[36,48]),
                       (3,[5,7,11,13],[20,21,22,26])]:
        rows.append(recipe(f"greatest-common-factor/kp{kp}","Find the GCF of ${a}$ and ${b}$.",
            "gcd(a,b)",dict(a=aa,b=bb),lambda a,b:math.gcd(a,b),
            "List factor pairs for each number, identify the factors shared by both lists, and select the largest shared factor.",
            "Which divisors appear in both factor lists?"))
    return rows


def common_multiples():
    rows=[recipe("least-common-multiple/kp1","Find the LCM of ${a}$ and ${b}$.",
        "lcm(a,b)",dict(a=range(2,11),b=[6,8,12]),lambda a,b:math.lcm(a,b),
        "List positive multiples of both numbers until a value appears in both lists; choose the first shared value.",
        "Where do the two lists of multiples first meet?")]
    rows.append(recipe("least-common-multiple/kp2","Find the LCM of ${a}$ and ${b}$.",
        "lcm(a,b)",dict(a=[5,7,11,13],b=[20,21,22,26]),lambda a,b:math.lcm(a,b),
        "Check whether the smaller number divides the larger. If so, the larger is the LCM. Otherwise these pairs are relatively prime, so multiply the two inputs.",
        "Does the smaller number divide the larger, or do they share no factor greater than unity?"))
    rows.append(recipe("least-common-multiple/kp3","Find the LCM of ${a}$, ${b}$, and $6$.",
        "lcm(lcm(a,b),6)",dict(a=range(2,10),b=[3,4,5]),lambda a,b:math.lcm(a,b,6),
        "Find a common multiple of the first two numbers, then take the least common multiple of that value and the third number.",
        "What is the first positive value divisible by all three inputs?"))
    return rows


def applications():
    return [
        recipe("gcf-lcm/kp1","Find the GCF of ${a}$ and $84$ using prime factorization.",
            "gcd(a,84)",dict(a=range(60,92,2)),lambda a:math.gcd(a,84),
            "Factor both inputs into primes. For each prime shared by the inputs, keep its smaller exponent; multiply those shared prime powers.",
            "Which prime powers are present in both factorizations?"),
        recipe("gcf-lcm/kp2","Find the LCM of ${a}$ and $36$ using prime factorization.",
            "lcm(a,36)",dict(a=range(21,37)),lambda a:math.lcm(a,36),
            "Factor both inputs into primes. Keep the larger exponent of every prime that occurs in either input, then multiply those prime powers.",
            "Which prime powers are needed to make a multiple of both inputs?"),
        recipe("gcf-lcm/kp3","Two bells ring together now. One rings every ${a}$ minutes and the other every $18$ minutes. In how many minutes will they next ring together?",
            "lcm(a,18)",dict(a=range(11,27)),lambda a:math.lcm(a,18),
            "The next shared ringing time must be a positive multiple of both intervals. Select their least common multiple.",
            "What kind of common multiple gives the next shared event?"),
    ]


def small_primes():
    primes=[2,3,5,7,11,13,17,19,23,29,31,37,41,43,47]
    expression="2"+"".join(f"+{q-p}*min(1,max(0,a-{p}+1))"
                           for p,q in zip(primes,primes[1:]))
    return [recipe("prime-composite-numbers/kp2",
        "What is the smallest prime greater than ${a}$?",expression,
        dict(a=range(2,32)),lambda a:next(p for p in primes if p>a),
        "Check successive integers above the given number. For each candidate, test divisibility by primes up to its square root. Stop at the first candidate with no such divisor.",
        "Which smaller primes must you test to rule out a composite candidate?")]


def square_recognition():
    def polynomial(name):
        return op('mul',*(op('sub',name,lit(n)) for n in [4,9,16,25]))
    indicator="min(1,abs((a-4)*(a-9)*(a-16)*(a-25)))"
    rules=[c('eq',op('mul',polynomial('a'),polynomial('b')),lit(0)),
           c('ne',op('add',polynomial('a'),polynomial('b')),lit(0))]
    return [recipe("perfect-squares/kp2",
        "Exactly one of ${a}$ and ${b}$ is a perfect square. Which number is it?",
        f"a*(1-{indicator})+b*{indicator}",
        dict(a=[3,4,5,9,10,16,17,25],b=[3,4,5,9,10,16,17,25]),
        lambda a,b:a if math.isqrt(a)**2==a else b,
        "Compare each candidate with the products of a whole number multiplied by itself. Select the candidate that equals one of these products.",
        "Can you express either candidate as a product of two equal whole numbers?",rules)]
