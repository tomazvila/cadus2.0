#!/usr/bin/env python3
"""Fail on every surviving mutant.

Usage: mutants.py cargo <mutants.out dir>
       mutants.py stryker <stryker json report>

A cargo-mutants outcome of `MissedMutant` is a survivor. A Stryker mutant with the
status `Survived` or `NoCoverage` is a survivor.
"""
import json
import os
import sys


def cargo(root):
    with open(os.path.join(root, "outcomes.json")) as handle:
        data = json.load(handle)
    missed = [o for o in data["outcomes"] if o["summary"] == "MissedMutant"]
    for outcome in missed:
        scenario = outcome["scenario"]["Mutant"]
        print(f"{scenario['file']}:{scenario['function']['function_name']} {scenario['genre']} survived")
    print(f"cargo mutants: {data['total_mutants']} mutants, {len(missed)} survived")
    return len(missed)


def stryker(path):
    with open(path) as handle:
        data = json.load(handle)
    survived = 0
    total = 0
    for name, item in data["files"].items():
        for mutant in item["mutants"]:
            total += 1
            if mutant["status"] in ("Survived", "NoCoverage"):
                survived += 1
                print(f"{name}:{mutant['location']['start']['line']} {mutant['mutatorName']} {mutant['status']}")
    print(f"stryker: {total} mutants, {survived} survived")
    return survived


def main() -> int:
    mode, path = sys.argv[1], sys.argv[2]
    count = cargo(path) if mode == "cargo" else stryker(path)
    return 1 if count else 0


if __name__ == "__main__":
    sys.exit(main())
