#!/usr/bin/env python
"""Write the U3 lint fixture trees under `crates/core/tests/fixtures/lint/`.

One minimal broken tree per lint code of `docs/reference/curriculum-1.0-spec.md`
section 5, translated from 1.0 `tests/test_graph.py:518-797`. `clean/` is a copy
of the 1.0 fixture `tests/fixtures/curriculum_mini`.

Run it with the 1.0 interpreter, then regenerate every `expected.json`:

    /home/deploy/dev/cadus/.venv/bin/python scripts/oracle/make_lint_fixtures.py
    for d in crates/core/tests/fixtures/lint/*/; do
        /home/deploy/dev/cadus/.venv/bin/python scripts/oracle/dump_lint_1_0.py \
            "$d" > "$d/expected.json"
    done
"""
from __future__ import annotations

import shutil
import sys
from pathlib import Path
from typing import Any

import yaml

MINI = Path("/home/deploy/dev/cadus/tests/fixtures/curriculum_mini")


# --- the 1.0 test builders (tests/test_graph.py:444-494), copied verbatim --- #


def _kp(exemplars: int = 1, key_prereqs: list[str] | None = None) -> dict[str, Any]:
    kp: dict[str, Any] = {
        "id": "kp1",
        "name": "k",
        "exemplars": [{"problem": "q", "answer": "a"} for _ in range(exemplars)],
    }
    if key_prereqs is not None:
        kp["key_prerequisites"] = key_prereqs
    return kp


def _topic(
    tid: str,
    *,
    prereqs: list[dict[str, Any]] | None = None,
    extra: list[dict[str, Any]] | None = None,
    kps: list[dict[str, Any]] | None = None,
    diag: bool = True,
    core: bool = True,
    answer_kind: str = "numeric",
) -> dict[str, Any]:
    t: dict[str, Any] = {
        "id": tid,
        "name": tid,
        "core": core,
        "difficulty": 0.3,
        "answer_kind": answer_kind,
        "expected_time_secs": 30,
        "prerequisites": prereqs or [],
        "knowledge_points": kps if kps is not None else [_kp()],
    }
    if extra is not None:
        t["encompassings_extra"] = extra
    if diag:
        t["diagnostic_exemplar"] = {"problem": "q", "answer": "a"}
    return t


def _write(root: Path, courses: list[dict[str, Any]], units: dict[str, dict[str, Any]]) -> Path:
    root.mkdir(parents=True, exist_ok=True)
    (root / "courses.yaml").write_text(yaml.safe_dump({"courses": courses}), encoding="utf-8")
    for rel, unit in units.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(yaml.safe_dump(unit), encoding="utf-8")
    return root


def _one_course(
    topics: list[dict[str, Any]], floor: list[str], module: str = "M"
) -> tuple[list[dict[str, Any]], dict[str, dict[str, Any]]]:
    courses = [{"id": "c", "name": "C", "order": 1, "mastery_floor": floor}]
    units = {"c/00.yaml": {"unit": "u", "course": "c", "module": module, "topics": topics}}
    return courses, units


# --- one fixture per lint code -------------------------------------------- #


def build(base: Path) -> None:
    # Start from an empty base, so a renamed course directory leaves no stale file
    # behind. Every tree below is written from scratch.
    if base.exists():
        shutil.rmtree(base)
    base.mkdir(parents=True)

    # 1. yaml — an unreadable unit file. Its course keeps no topic.
    root = _write(base / "yaml", *_one_course([], floor=[]))
    (root / "c" / "00.yaml").write_text('unit: "u\ncourse: c\n', encoding="utf-8")

    # 2. schema — an answer_kind outside the enum drops the whole file.
    _write(base / "schema", *_one_course([_topic("a", answer_kind="banana")], floor=[]))

    # 3. weight_out_of_range — a prerequisite weight above 1.0.
    _write(
        base / "weight_out_of_range",
        *_one_course([_topic("a"), _topic("b", prereqs=[{"id": "a", "weight": 1.5}])], floor=[]),
    )

    # 4. missing_course_dir — a declared course with no directory (advisory).
    _write(
        base / "missing_course_dir",
        [
            {"id": "c", "name": "C", "order": 1, "mastery_floor": ["a"]},
            {"id": "ghostcourse", "name": "Ghost", "order": 2},
        ],
        {"c/00.yaml": {"unit": "u", "course": "c", "module": "M", "topics": [_topic("a")]}},
    )

    # 5. empty_course — a directory with no unit file (advisory). `.gitkeep` keeps
    #    the empty directory in git and stays invisible to the `*.yaml` glob.
    root = _write(
        base / "empty_course",
        [
            {"id": "c", "name": "C", "order": 1, "mastery_floor": ["a"]},
            {"id": "hollowcourse", "name": "Hollow", "order": 2},
        ],
        {"c/00.yaml": {"unit": "u", "course": "c", "module": "M", "topics": [_topic("a")]}},
    )
    (root / "hollowcourse").mkdir(exist_ok=True)
    (root / "hollowcourse" / ".gitkeep").write_text("", encoding="utf-8")

    # 6. duplicate_topic_id — lint keeps the first definition and reports the second.
    #    The tree is the two-course shape of 1.0
    #    `test_lint_reachability_skipped_on_duplicate_topic_id`: c2 holds no root
    #    and declares no floor, so the reachability rule WOULD flag `b`. It is
    #    skipped, and the duplicate is the only finding. A port that forgets the
    #    skip reports `unreachable_from_floor` here.
    _write(
        base / "duplicate_topic_id",
        [
            {"id": "c1", "name": "C1", "order": 1, "mastery_floor": ["a"]},
            {"id": "c2", "name": "C2", "order": 2},
        ],
        {
            "c1/00.yaml": {
                "unit": "u1",
                "course": "c1",
                "module": "M1",
                "topics": [_topic("a"), _topic("a")],
            },
            "c2/00.yaml": {
                "unit": "u2",
                "course": "c2",
                "module": "M2",
                "topics": [_topic("b", prereqs=[{"id": "a", "weight": 0.5}])],
            },
        },
    )

    # 7. missing_ref — all three messages: prerequisite, encompassings_extra, key.
    _write(
        base / "missing_ref",
        *_one_course(
            [
                _topic("a"),
                _topic(
                    "b",
                    prereqs=[{"id": "ghost", "weight": 0.5}],
                    extra=[{"id": "phantom", "weight": 0.3}],
                    kps=[_kp(key_prereqs=["nowhere"])],
                ),
            ],
            floor=["a"],
        ),
    )

    # 8. cycle — a three-topic loop, so the arrow join shows two arrows.
    _write(
        base / "cycle",
        *_one_course(
            [
                _topic("a", prereqs=[{"id": "b", "weight": 0.5}]),
                _topic("b", prereqs=[{"id": "c", "weight": 0.5}]),
                _topic("c", prereqs=[{"id": "a", "weight": 0.5}]),
            ],
            floor=[],
        ),
    )

    # 9. no_kp — a topic with an empty knowledge_points list.
    _write(base / "no_kp", *_one_course([_topic("a", kps=[])], floor=["a"]))

    # 10. no_exemplar — a knowledge point with an empty exemplars list.
    _write(base / "no_exemplar", *_one_course([_topic("a", kps=[_kp(exemplars=0)])], floor=["a"]))

    # 11. missing_diagnostic_exemplar — the field is absent.
    _write(
        base / "missing_diagnostic_exemplar",
        *_one_course([_topic("a", diag=False)], floor=["a"]),
    )

    # 12. key_prereq_not_ancestor — `c` is a sibling of `b`, not an ancestor.
    _write(
        base / "key_prereq_not_ancestor",
        *_one_course(
            [
                _topic("a"),
                _topic("c", prereqs=[{"id": "a", "weight": 0.5}]),
                _topic("b", prereqs=[{"id": "a", "weight": 0.5}], kps=[_kp(key_prereqs=["c"])]),
            ],
            floor=["a"],
        ),
    )

    # 13. noncore_ancestor_of_core — two core dependents, so the message carries
    #     the "(+1 more)" tail and the context lists both.
    _write(
        base / "noncore_ancestor_of_core",
        *_one_course(
            [
                _topic("base", core=False),
                _topic("top", prereqs=[{"id": "base", "weight": 0.5}]),
                _topic("top2", prereqs=[{"id": "base", "weight": 0.5}]),
            ],
            floor=["base"],
        ),
    )

    # 14. module_inconsistent — one module name over two courses.
    _write(
        base / "module_inconsistent",
        [
            {"id": "c1", "name": "C1", "order": 1, "mastery_floor": ["a"]},
            {"id": "c2", "name": "C2", "order": 2, "mastery_floor": ["a"]},
        ],
        {
            "c1/00.yaml": {"unit": "u1", "course": "c1", "module": "Shared", "topics": [_topic("a")]},
            "c2/00.yaml": {
                "unit": "u2",
                "course": "c2",
                "module": "Shared",
                "topics": [_topic("b", prereqs=[{"id": "a", "weight": 0.5}])],
            },
        },
    )

    # 14b. module_inconsistent — the other message: a blank module name.
    _write(
        base / "module_inconsistent_empty_name",
        *_one_course([_topic("a")], floor=["a"], module="   "),
    )

    # 15. mastery_floor_ambiguous — a course with both floor forms.
    _write(
        base / "mastery_floor_ambiguous",
        [
            {"id": "c1", "name": "C1", "order": 1},
            {
                "id": "c2",
                "name": "C2",
                "order": 2,
                "mastery_floor": ["z"],
                "mastery_floor_course": "c1",
            },
        ],
        {
            "c1/00.yaml": {"unit": "u1", "course": "c1", "module": "M1", "topics": [_topic("a")]},
            "c2/00.yaml": {
                "unit": "u2",
                "course": "c2",
                "module": "M2",
                "topics": [_topic("z"), _topic("b", prereqs=[{"id": "a", "weight": 0.5}])],
            },
        },
    )

    # 16. unreachable_from_floor — c2 declares no floor and holds no root.
    _write(
        base / "unreachable_from_floor",
        [
            {"id": "c1", "name": "C1", "order": 1, "mastery_floor": ["a"]},
            {"id": "c2", "name": "C2", "order": 2},
        ],
        {
            "c1/00.yaml": {"unit": "u1", "course": "c1", "module": "M1", "topics": [_topic("a")]},
            "c2/00.yaml": {
                "unit": "u2",
                "course": "c2",
                "module": "M2",
                "topics": [_topic("b", prereqs=[{"id": "a", "weight": 0.5}])],
            },
        },
    )

    # `empty` — no courses.yaml at all.
    root = base / "empty"
    root.mkdir(parents=True, exist_ok=True)
    (root / "README.txt").write_text(
        "No courses.yaml here. The tree is the `empty` lint fixture.\n", encoding="utf-8"
    )

    # `clean` — the 1.0 fixture `tests/fixtures/curriculum_mini`, 0 findings.
    clean = base / "clean"
    if clean.exists():
        shutil.rmtree(clean)
    shutil.copytree(MINI, clean)


def main() -> int:
    base = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("crates/core/tests/fixtures/lint")
    build(base)
    print(f"wrote {len(list(base.iterdir()))} fixture trees under {base}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
