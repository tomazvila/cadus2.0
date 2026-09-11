"""Direct substitution and Boolean composition, independent of signcase formulas."""
import re
from semantic import OPS, math, scalar, relation


def is_label(problem):
    return problem.endswith('?')


def candidate(problem):
    if 'origin' in problem: return 0
    values=[s.split('=')[1] for s in math(problem) if re.fullmatch(r'x=[-\d]+',s)]
    assert len(values)==1,problem
    return scalar(values[0])


def comparisons(problem):
    return [s for s in math(problem) if re.search(r'<=|>=|<|>',s)]


def checks(problem):
    x=candidate(problem)
    result=[]
    for equation in comparisons(problem):
        left,op,right=re.split(r'(<=|>=|<|>)',equation)
        a,b=scalar(left,x),scalar(right,x)
        result.append((a,op,b,OPS[op](a,b)))
    assert result,problem
    return result


def reconstruct(problem):
    truths=[r[3] for r in checks(problem)]
    result=any(truths) if ' or ' in problem else all(truths)
    return 'yes' if result else 'no'


def sketch(problem):
    rows=checks(problem)
    substitutions='; '.join(f'${a} {op} {b}$ is {str(truth).lower()}' for a,op,b,truth in rows)
    if 'origin' in problem:
        return ('Substitute both coordinates of the origin as zero: '+substitutions+
                '. Shade the side containing the test point precisely when the comparison is true.')
    solving=''
    if problem.startswith('Solve'):
        op,bound=relation(comparisons(problem)[0])
        solving=f'Isolating x gives $x {op} {bound}$. '
    rule='Either comparison suffices for a union.' if ' or ' in problem else 'Every comparison must hold for an intersection.'
    return solving+'Substitute the candidate in the original inequality: '+substitutions+'. '+rule


def family(problem):
    # The test point and actual conditions matter; equivalent prose does not.
    return ('membership',candidate(problem),tuple(s.replace(' ','') for s in comparisons(problem)),
            'or' if ' or ' in problem else 'and')


def verify(problem,answer,solution=None):
    assert answer==reconstruct(problem),(problem,answer,reconstruct(problem))
    if solution is not None:
        assert len(solution)>70 and 'Substitute' in solution
        # Every claimed numerical comparison must be true or false as written.
        for eq,word in re.findall(r'\$([^$]+)\$ is (true|false)',solution):
            a,op,b=re.split(r'(<=|>=|<|>)',math('$'+eq+'$')[0])
            assert OPS[op](scalar(a),scalar(b))==(word=='true')
        if problem.startswith('Solve'):
            solved=next(s for s in math(solution) if s.startswith('x '))
            assert relation(solved)==relation(comparisons(problem)[0])
    return family(problem)
