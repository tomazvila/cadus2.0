"""Shared code of the 1.0 oracle scripts.

Every oracle that folds a committed stream reads the same options, points the
1.0 loader at the same trees, lists the same fixtures, and writes the same index
shape. This module holds that code once. It imports nothing from the 1.0 code
base, so a script imports it before it points the environment at 1.0.

The argument helpers add their options in the order the scripts always used, so
the `--help` text of every script stays the same.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from datetime import UTC, datetime

#: The fixture directory of this repository, found from this file's own path.
FIXTURES = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "..",
        "crates",
        "core",
        "tests",
        "fixtures",
        "events",
    )
)

#: The default 1.0 trees: the 2.0 curriculum and the 1.0 config file.
DEFAULT_CURRICULUM = "/home/deploy/dev/cadus2.0/curriculum"
DEFAULT_CONFIG = "/home/deploy/dev/cadus/config.yaml"

#: The pinned `now` of every fold, so no wall clock enters a result.
DEFAULT_NOW = "2000-01-01T00:00:00+00:00"

#: The daily XP goal of every fold (`config.yaml`).
DEFAULT_GOAL = 40

#: The file name of a numbered stream.
STREAM_NAME = re.compile(r"stream_\d+\.jsonl")


def canonical(obj: object) -> str:
    """Canonical JSON: sorted keys, compact separators, UTF-8, no NaN."""
    return json.dumps(
        obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False
    )


def sha256_of(text: str) -> str:
    """The hex sha256 of the UTF-8 bytes of `text`."""
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def parity_blob(model, keep_built_from_ts: bool = False) -> str:
    """The parity blob: the canonical model with `built_from_ts` removed.

    The model goes through the pydantic JSON encoder first, so datetimes,
    dates, and enums render exactly as 1.0 persists them.
    """
    payload = json.loads(model.model_dump_json())
    if not keep_built_from_ts:
        payload.pop("built_from_ts", None)
    return canonical(payload)


def parity_digest(model) -> str:
    """The sha256 of the parity blob of `model`."""
    return sha256_of(parity_blob(model))


def add_code_base_arguments(ap: argparse.ArgumentParser) -> None:
    """Add `--curriculum` and `--config`, the two trees the 1.0 loader reads."""
    ap.add_argument("--curriculum", default=DEFAULT_CURRICULUM)
    ap.add_argument("--config", default=DEFAULT_CONFIG)


def add_now_argument(ap: argparse.ArgumentParser) -> None:
    """Add `--now`, the pinned instant of the fold."""
    ap.add_argument("--now", default=DEFAULT_NOW)


def add_goal_argument(ap: argparse.ArgumentParser) -> None:
    """Add `--goal`, the daily XP goal of the fold."""
    ap.add_argument("--goal", type=int, default=DEFAULT_GOAL)


def point_at_code_base(args: argparse.Namespace) -> None:
    """Point the 1.0 loader at the trees of `--curriculum` and `--config`."""
    os.environ["CADUS_CURRICULUM"] = args.curriculum
    os.environ["CADUS_CONFIG"] = args.config


def parse_stream_oracle_args(doc: str) -> argparse.Namespace:
    """Parse the options of a stream oracle and point the loader at 1.0.

    The options are `--fixtures`, `--out`, the code base pair, `--now`, and
    `--goal`, in that order.
    """
    ap = argparse.ArgumentParser(description=doc)
    ap.add_argument("--fixtures", default=FIXTURES)
    ap.add_argument("--out", default=None)
    add_code_base_arguments(ap)
    add_now_argument(ap)
    add_goal_argument(ap)
    args = ap.parse_args()
    point_at_code_base(args)
    return args


def load_1_0():
    """Load the 1.0 config and graph. Call `point_at_code_base` first."""
    from cadus.loader import load_config, load_graph

    return load_config(), load_graph()


def parse_now(text: str) -> datetime:
    """The aware instant of `--now`. A naive value is UTC."""
    now = datetime.fromisoformat(text)
    if now.tzinfo is None:
        now = now.replace(tzinfo=UTC)
    return now


def stream_number(name: str) -> int:
    """The number of a `stream_N.jsonl` file name."""
    return int(name.removeprefix("stream_").removesuffix(".jsonl"))


def stream_names(fixtures: str) -> list[str]:
    """The `stream_N.jsonl` files of `fixtures`, in numeric order."""
    return sorted(
        (name for name in os.listdir(fixtures) if STREAM_NAME.fullmatch(name)),
        key=stream_number,
    )


def iter_rows(path: str):
    """The JSON objects of a JSONL file, one per non-blank line, in file order."""
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                yield json.loads(line)


def read_rows(path: str) -> list[dict]:
    """The JSON objects of a JSONL file, as a list."""
    return list(iter_rows(path))


def read_events(path: str, validate_event) -> list:
    """The events of a JSONL stream, each validated through `validate_event`."""
    return [validate_event(row) for row in read_rows(path)]


def write_index(out: str, index: dict) -> None:
    """Write `index` as indented JSON with sorted keys, and say where it went."""
    with open(out, "w", encoding="utf-8") as handle:
        handle.write(json.dumps(index, indent=2, sort_keys=True) + "\n")
    print(f"wrote {out}")
