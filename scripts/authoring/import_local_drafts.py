#!/usr/bin/env python3
"""Store explicit local drafts as pending content with zero API spend.

The helper reads one manifest, validates every draft, serves the drafts through
the loopback endpoint, and runs the normal `cadus-worker author` pass once per
kind. The worker applies every production content check and writes only
`pending` rows. The helper never approves a row and never contacts a model API.
DATABASE_URL selects the target database. The helper stops its server after the
worker exits, on every path.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import threading
from pathlib import Path

from draft_endpoint import server_for

KINDS = ("template", "teach", "hint_ladder", "diagnosis")
KEY_SHAPE = re.compile(r"^[a-z0-9-]+/[a-z0-9-]+$")
ROW_FIELDS = {"kp_id", "kind", "arguments"}
REPORT_LINE = re.compile(
    r"^(?P<kind>[a-z_]+): stored (?P<stored>\d+) skipped (?P<skipped>\d+) "
    r"declined (?P<declined>\d+) calls (?P<calls>\d+) alerts (?P<alerts>\d+)$"
)
REPORTED_LINE = re.compile(r"reported: (?P<micros>\d+) micro-USD")
PLACEHOLDER_KEY = "operator-draft-local-only"
MODEL_ID = "operator-draft-v1"
EXIT_REFUSED = 2


class DraftError(ValueError):
    """A manifest or a draft that the helper refuses before any process starts."""


def load_document(manifest: Path):
    """Read the manifest and every draft file it names; return (document, rows)."""
    document = json.loads(manifest.read_text())
    if isinstance(document, list):
        return {}, document
    if not isinstance(document, dict) or not isinstance(document.get("files"), list):
        raise DraftError("the manifest is neither a draft list nor an object with a 'files' list")
    rows = []
    for name in document["files"]:
        if not isinstance(name, str) or Path(name).is_absolute() or ".." in Path(name).parts:
            raise DraftError(f"invalid manifest file entry: {name!r}")
        part = json.loads((manifest.parent / name).read_text())
        if not isinstance(part, list):
            raise DraftError(f"{name} is not a draft list")
        rows.extend(part)
    return document, rows


def validate(document, rows):
    """Return {(kp_id, kind): arguments}; refuse the first invalid draft."""
    drafts = {}
    for index, row in enumerate(rows):
        if not isinstance(row, dict) or set(row) != ROW_FIELDS:
            raise DraftError(f"draft {index}: a draft holds exactly kp_id, kind and arguments")
        kp_id, kind, arguments = row["kp_id"], row["kind"], row["arguments"]
        if not isinstance(kp_id, str) or not KEY_SHAPE.match(kp_id):
            raise DraftError(f"draft {index}: kp_id {kp_id!r} is not a serving key <topic_id>/<kp_id>")
        if kind not in KINDS:
            raise DraftError(f"draft {index}: kind {kind!r} is not one of {', '.join(KINDS)}")
        if not isinstance(arguments, dict) or not arguments:
            raise DraftError(f"draft {index}: arguments must be a non-empty JSON object")
        if (kp_id, kind) in drafts:
            raise DraftError(f"draft {index}: duplicate draft key {kp_id} {kind}")
        drafts[(kp_id, kind)] = arguments
    if not drafts:
        raise DraftError("the manifest names no draft")
    declared_kinds = document.get("kinds")
    if declared_kinds is not None:
        outside = sorted({kind for _, kind in drafts} - set(declared_kinds))
        if outside:
            raise DraftError(f"drafts of kind {', '.join(outside)} are outside the manifest 'kinds'")
    declared_count = document.get("knowledge_points")
    if declared_count is not None:
        actual = len({kp_id for kp_id, _ in drafts})
        if actual != declared_count:
            raise DraftError(f"the manifest declares {declared_count} knowledge points; the drafts name {actual}")
    return drafts


def parse_report(stdout):
    """Sum the per-kind report lines of one worker run."""
    totals = {"stored": 0, "skipped": 0, "declined": 0, "calls": 0}
    declines = []
    for line in stdout.splitlines():
        match = REPORT_LINE.match(line.strip())
        if match:
            for field in totals:
                totals[field] += int(match.group(field))
        elif line.startswith("declined "):
            declines.append(line.strip())
    reported = [int(match.group("micros")) for match in REPORTED_LINE.finditer(stdout)]
    return totals, declines, reported


def child_environment(base_url, curriculum):
    """The worker environment: loopback endpoint, placeholder key, zero-cost model id."""
    environment = os.environ.copy()
    environment.update({
        "OPENAI_BASE_URL": base_url,
        "OPENAI_API_KEY": PLACEHOLDER_KEY,
        "OPENAI_MODEL": MODEL_ID,
        "CADUS_CURRICULUM": str(curriculum),
    })
    return environment


def worker_command(args, kind, keys):
    command = [args.worker, "author", "--kind", kind,
               "--budget-usd", str(args.budget_usd),
               "--request-reserve-usd", str(args.request_reserve_usd),
               "--concurrency", str(args.concurrency)]
    if args.dry_run:
        command.append("--dry-run")
    if getattr(args, "missing_only", False):
        command.append("--missing-only")
    for key in keys:
        command.extend(["--kp", key])
    return command


def run(args, drafts):
    """Serve the drafts and run one worker pass per kind; return the exit code."""
    root = Path(__file__).resolve().parents[2]
    curriculum = Path(args.curriculum) if args.curriculum else root / "curriculum"
    server = server_for(0, drafts)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    totals = {"stored": 0, "skipped": 0, "declined": 0, "calls": 0}
    declines, reported, worst = [], [], 0
    try:
        environment = child_environment(f"http://127.0.0.1:{server.server_address[1]}/v1", curriculum)
        for kind in KINDS:
            keys = list(dict.fromkeys(kp_id for kp_id, draft_kind in drafts if draft_kind == kind))
            if not keys:
                continue
            result = subprocess.run(worker_command(args, kind, keys), env=environment, cwd=root,
                                    capture_output=True, text=True, check=False)
            sys.stdout.write(result.stdout)
            sys.stderr.write(result.stderr)
            worst = max(worst, result.returncode)
            part, part_declines, part_reported = parse_report(result.stdout)
            for field in totals:
                totals[field] += part[field]
            declines.extend(part_declines)
            reported.extend(part_reported)
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    print("local drafts: stored {stored} skipped {skipped} declined {declined} calls {calls}".format(**totals))
    for line in declines:
        print(line)
    if any(reported):
        print("local drafts: the worker reported a nonzero model cost", file=sys.stderr)
        return 1
    print(f"local drafts: model cost reported {sum(reported)} micro-USD")
    if worst == 0 and totals["declined"] > 0:
        return 1
    return worst


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--manifest", required=True, help="a manifest with a 'files' list, or a plain draft list")
    parser.add_argument("--worker", required=True, help="the cadus-worker binary")
    parser.add_argument("--curriculum", default=None, help="the curriculum tree; the default is ./curriculum")
    parser.add_argument("--budget-usd", default="5")
    parser.add_argument("--request-reserve-usd", default="0.50")
    parser.add_argument("--concurrency", type=int, default=4)
    parser.add_argument("--missing-only", action="store_true", help="skip every pair with pending or approved content")
    parser.add_argument("--dry-run", action="store_true", help="print the worker plan and write nothing")
    args = parser.parse_args()
    if not Path(args.worker).is_file():
        print(f"local drafts: no worker binary at {args.worker}", file=sys.stderr)
        return EXIT_REFUSED
    try:
        document, rows = load_document(Path(args.manifest))
        drafts = validate(document, rows)
    except (DraftError, OSError, json.JSONDecodeError) as error:
        print(f"local drafts: refused before any process started: {error}", file=sys.stderr)
        return EXIT_REFUSED
    print(f"local drafts: {len(drafts)} drafts for {len({kp for kp, _ in drafts})} knowledge points")
    return run(args, drafts)


if __name__ == "__main__":
    raise SystemExit(main())
