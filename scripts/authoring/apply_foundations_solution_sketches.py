#!/usr/bin/env python3
"""Refuse coarse-family solution-sketch curriculum mutation.

An evaluated numeric equality proves the result, but it does not prove that a
generic explanation teaches the knowledge point's declared method. This
command stays fail-closed until reviewed, KP-specific recipes are available.
"""
import sys


def main() -> int:
    print(
        "REFUSED: solution-sketch mutation requires reviewed KP-specific recipes",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
