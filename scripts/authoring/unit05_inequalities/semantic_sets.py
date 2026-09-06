"""Solve bounded sets independently using affine preimages over exact rationals."""
import re
from semantic import Q, OPS, affine, math, scalar


def parts(problem):
    return next(s for s in math(problem) if re.search(r'<|>',s))


def preimage(inner, low, high, lc, hc):
    m,c=affine(inner)
    assert m, 'Constant/cancelling absolute value or chain'
    lo,hi=(low-c)/m,(high-c)/m
    return (lo,hi,lc,hc) if m>0 else (hi,lo,hc,lc)


def bounds(problem):
    eq=parts(problem)
    if '|' in eq:
        return absolute_bounds(eq)
    lo,op1,inner,op2,hi=re.split(r'(<=|>=|<|>)',eq)
    assert op1 in ('<','<=') and op2 in ('<','<=')
    return preimage(inner,scalar(lo),scalar(hi),op1=='<=',op2=='<=')


def absolute_data(eq):
    left,op,right=re.split(r'(<=|>=|<|>)',eq)
    if '|' not in left:
        left,right=right,left
        op={'<':'>','<=':'>=','>':'<','>=':'<='}[op]
    prefix,inner,suffix=left.split('|')
    # Replace absolute value with a fresh linear scalar to isolate its magnitude.
    m,c=affine(prefix+'x'+suffix)
    assert m>0 and op in ('<','<='), 'Only bounded positive-magnitude cases'
    radius=(scalar(right)-c)/m
    assert radius>0, 'Degenerate family'
    return inner,m,c,radius,op


def absolute_bounds(eq):
    inner,_,_,radius,op=absolute_data(eq)
    return preimage(inner,-radius,radius,op=='<=',op=='<=')


def reconstruct(problem):
    lo,hi,lc,hc=bounds(problem)
    assert lo<hi
    if 'interval notation' in problem:
        return f'{"[" if lc else "("}{lo}, {hi}{"]" if hc else ")"}'
    return f'{lo} {"<=" if lc else "<"} x {"<=" if hc else "<"} {hi}'


def sketch(problem):
    lo,hi,lc,hc=bounds(problem)
    interval=f'{lo} {"<=" if lc else "<"} x {"<=" if hc else "<"} {hi}'
    eq=parts(problem)
    if '|' in eq:
        inner,m,c,radius,op=absolute_data(eq)
        inner_m,inner_c=affine(inner)
        method=(f'Isolate the magnitude by subtracting ${c}$ and dividing by positive ${m}$: '
                f'$|{inner}| {op} {radius}$. Thus $-{radius} {op} {inner} {op} {radius}$. '
                f'Subtract ${inner_c}$ throughout and divide by ${inner_m}$'
                + (', reversing both comparisons.' if inner_m<0 else ', preserving both comparisons.'))
    elif 'interval notation' in problem:
        method='Read the lower and upper bounds. Use a square bracket at an included endpoint and a round bracket at an excluded endpoint.'
    else:
        method='Apply the inverse operations to all three parts. A negative divisor reverses both comparisons; reorder the endpoints from least to greatest.'
    return f'{method} The boundary calculation gives ${interval}$.'


def contains(problem,x):
    """Pointwise truth in the original expression; separate from inverse solving."""
    eq=parts(problem)
    if '|' in eq:
        prefix,inner,suffix=eq.split('|')
        eq=prefix+f'({abs(scalar(inner,x))})'+suffix
    terms=re.split(r'(<=|>=|<|>)',eq)
    return all(OPS[terms[i]](scalar(terms[i-1],x),scalar(terms[i+1],x))
               for i in range(1,len(terms),2))


def verify(problem,answer,solution=None):
    assert answer==reconstruct(problem), (problem,answer,reconstruct(problem))
    lo,hi,lc,hc=bounds(problem)
    for x in [lo-1,lo,(3*lo+hi)/4,(lo+hi)/2,(lo+3*hi)/4,hi,hi+1]:
        wanted=(x>lo or lc and x==lo) and (x<hi or hc and x==hi)
        assert contains(problem,x)==wanted, (problem,x)
    if solution is not None:
        assert len(solution)>70 and 'boundary' in solution
        for result in (s for s in math(solution) if re.search(r'<|>',s)):
            assert bounds(f'${result}$')==(lo,hi,lc,hc)
    return ('bounded',parts(problem).replace(' ',''))
