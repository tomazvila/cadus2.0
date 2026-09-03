#!/usr/bin/env python3
"""Fail on every public Rust item that no other line of the workspace names.

Usage: rust_dead.py < list-of-rust-paths

`cargo clippy -D warnings` already refuses a private item without a use. A `pub` item
is exempt from that lint, so this check counts the uses of every `pub` name across the
whole workspace: a name that appears on its definition line only is dead.
"""
import collections
import re
import sys

DEF = re.compile(
    r"^\s*pub(?:\([^)]*\))?\s+(?:async\s+)?(?:unsafe\s+)?"
    r"(fn|struct|enum|type|const|static|trait|mod)\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.M,
)
WORD = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")


def main() -> int:
    paths = [line.strip() for line in sys.stdin if line.strip()]
    texts = {}
    for path in paths:
        with open(path) as handle:
            texts[path] = handle.read()
    counts = collections.Counter()
    for text in texts.values():
        counts.update(WORD.findall(text))
    dead = []
    for path, text in texts.items():
        for match in DEF.finditer(text):
            name = match.group(2)
            if counts[name] <= 1:
                line = text[: match.start()].count("\n") + 1
                dead.append((path, line, match.group(1), name))
    for path, line, kind, name in sorted(dead):
        print(f"{path}:{line} pub {kind} {name} has no use")
    print(f"rust dead code: {len(dead)} unused public items")
    return 1 if dead else 0


if __name__ == "__main__":
    sys.exit(main())
