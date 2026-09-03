#!/usr/bin/env python3
"""Fail on every tracked source file with 500 lines or more.

Usage: loc.py [--limit N] < list-of-paths

The check counts physical lines, the same count that `wc -l` prints.
"""
import sys


def main() -> int:
    limit = 500
    args = sys.argv[1:]
    if len(args) == 2 and args[0] == "--limit":
        limit = int(args[1])
    paths = [line.strip() for line in sys.stdin if line.strip()]
    long_files = []
    for path in paths:
        with open(path, "rb") as handle:
            lines = sum(1 for _ in handle)
        if lines >= limit:
            long_files.append((lines, path))
    for lines, path in sorted(long_files, reverse=True):
        print(f"{lines:6d} {path}")
    print(f"loc: {len(paths)} files, {len(long_files)} at or over {limit} lines")
    return 1 if long_files else 0


if __name__ == "__main__":
    sys.exit(main())
