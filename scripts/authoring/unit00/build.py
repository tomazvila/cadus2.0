"""Build local candidates only. Worker validation is required before publication."""
import argparse
import json
from pathlib import Path

import yaml
import arithmetic
import operations
import number_theory
from common import arguments


def recipes():
    groups=[arithmetic.roots,arithmetic.addition,arithmetic.subtraction,arithmetic.words,
            operations.place,operations.multiplication,operations.division,
            operations.grouping,operations.estimation,number_theory.comparisons,
            number_theory.common_factors,number_theory.common_multiples,
            number_theory.applications,number_theory.small_primes,number_theory.square_recognition]
    return [row for group in groups for row in group()]


def build(root):
    unit=yaml.safe_load((root/"curriculum/foundations/00-arithmetic-core.yaml").read_text())
    by_key={t["id"]+"/"+k["id"]: (t,k) for t in unit["topics"] for k in t["knowledge_points"]}
    out=[]
    for item in recipes():
        key=item["kp_id"]
        topic,kp=by_key[key]
        forbidden=[e for k in topic["knowledge_points"] for e in k["exemplars"]]
        if "diagnostic_exemplar" in topic:
            forbidden.append(topic["diagnostic_exemplar"])
        args=arguments(item,forbidden)
        out.append(dict(kp_id=key,kind="template",arguments=args))
    return out, sorted(set(by_key)-{r["kp_id"] for r in out})


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root",type=Path,default=Path.cwd())
    parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args()
    rows,missing=build(args.root)
    args.output.write_text(json.dumps(rows,indent=2)+"\n")
    print(json.dumps(dict(candidates=len(rows),keys=len({r['kp_id'] for r in rows}),
                          unrepresented=missing),indent=2))


if __name__ == "__main__":
    main()
