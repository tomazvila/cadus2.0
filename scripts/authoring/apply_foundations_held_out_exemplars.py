#!/usr/bin/env python3
"""Refuse coarse-family held-out curriculum mutation.

Arithmetic equivalence, answer range, and operand bounds do not prove that a
generated item obeys its knowledge point's semantic constraint. This command
stays fail-closed until reviewed, KP-specific recipes replace generic operand
redraws.
"""
import sys


def main() -> int:
    print(
        "REFUSED: held-out curriculum generation requires reviewed KP-specific recipes",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
