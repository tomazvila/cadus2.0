"""Shared dry-run/write command flow for explicit Foundations recipe scripts."""
import argparse
import sys
from pathlib import Path

from foundations_curriculum_patch import Rejection, apply_solution_sketches, insert_exemplars


def run_recipe_cli(unit_file,sketches,recipes,summary,*,sketch_summary=None,
                   sketch_refusal="REFUSED",recipe_refusal="REFUSED",description=None):
    parser=argparse.ArgumentParser(description=description)
    parser.add_argument("--curriculum",default=unit_file)
    parser.add_argument("--write",action="store_true")
    args=parser.parse_args()
    path=Path(args.curriculum)
    try:
        _,sketched=apply_solution_sketches(path,sketches,write=args.write)
    except Rejection as error:
        print(f"{sketch_refusal}: {error}",file=sys.stderr)
        return 1
    verb="written" if args.write else "planned"
    if sketch_summary is not None:
        print(sketch_summary(len(sketched),verb))
    try:
        _,applied=insert_exemplars(path,recipes,write=args.write)
    except Rejection as error:
        print(f"{recipe_refusal}: {error}",file=sys.stderr)
        return 1
    count=sum(len(recipes[key]) for key in applied)
    print(summary(len(sketched),len(applied),count,verb))
    return 0
