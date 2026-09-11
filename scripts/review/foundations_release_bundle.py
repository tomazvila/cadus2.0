#!/usr/bin/env python3
"""Build or verify the exact, ordered Foundations production-import bundle."""
import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

FIELDS = {"kp_id", "kind", "arguments"}
EXPECTED = {"template": 809, "teach": 809, "hint_ladder": 809}
ORDER = ("01-templates.json", "02-teach.json", "03-instruction.json")


class Refused(ValueError):
    """A source or bundle that is unsafe to import."""


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def digest_bytes(value):
    return hashlib.sha256(value).hexdigest()


def digest_json(value):
    return digest_bytes(canonical(value).encode())


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise Refused(f"{path}: {error}") from error


def read_source(path):
    """Read a list or manifest and return rows plus raw-file fingerprints."""
    document = read_json(path)
    files = []
    if isinstance(document, list):
        rows = document
        files.append(path)
    elif isinstance(document, dict) and isinstance(document.get("files"), list):
        rows = []
        files.append(path)
        for name in document["files"]:
            member = Path(name) if isinstance(name, str) else Path("/")
            if member.is_absolute() or ".." in member.parts:
                raise Refused(f"{path}: invalid member {name!r}")
            member = path.parent / member
            part = read_json(member)
            if not isinstance(part, list):
                raise Refused(f"{member}: expected a JSON list")
            rows.extend(part)
            files.append(member)
    else:
        raise Refused(f"{path}: expected a JSON list or manifest with files")
    evidence = [{"path": str(item.resolve()), "sha256": digest_bytes(item.read_bytes())}
                for item in files]
    return rows, evidence


def validate_rows(rows, label, kinds):
    seen = set()
    for number, row in enumerate(rows):
        if not isinstance(row, dict) or set(row) != FIELDS:
            raise Refused(f"{label} row {number}: expected exactly {sorted(FIELDS)}")
        key, kind, arguments = row["kp_id"], row["kind"], row["arguments"]
        if not isinstance(key, str) or key.count("/") != 1:
            raise Refused(f"{label} row {number}: invalid kp_id")
        if kind not in kinds or not isinstance(arguments, dict) or not arguments:
            raise Refused(f"{label} row {number}: invalid kind or arguments")
        pair = (key, kind)
        if pair in seen:
            raise Refused(f"{label}: duplicate {key} {kind}")
        seen.add(pair)


def git_head(root):
    result = subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"],
                            check=True, capture_output=True, text=True)
    return result.stdout.strip()


def prepare(templates, teach, instruction):
    sources = []
    template_rows, evidence = read_source(templates)
    sources.append({"role": "templates", "files": evidence})
    teach_rows, evidence = read_source(teach)
    sources.append({"role": "whole_course_teach", "files": evidence})
    instruction_rows, evidence = read_source(instruction)
    sources.append({"role": "remaining_teach_and_hints", "files": evidence})
    validate_rows(template_rows, "templates", {"template"})
    validate_rows(teach_rows, "whole-course teach", {"teach"})
    validate_rows(instruction_rows, "remaining instruction", {"teach", "hint_ladder"})
    groups = [template_rows, teach_rows, instruction_rows]
    pairs = [(row["kp_id"], row["kind"]) for rows in groups for row in rows]
    if len(pairs) != len(set(pairs)):
        raise Refused("the ordered sources overlap on a (kp_id, kind) pair")
    counts = {kind: sum(row["kind"] == kind for rows in groups for row in rows)
              for kind in EXPECTED}
    if counts != EXPECTED:
        raise Refused(f"content counts {counts}, expected {EXPECTED}")
    keysets = {kind: {row["kp_id"] for rows in groups for row in rows
                      if row["kind"] == kind} for kind in EXPECTED}
    if any(len(keys) != 809 for keys in keysets.values()) or len(set(map(frozenset, keysets.values()))) != 1:
        raise Refused("template, teach, and hint keys must be the same 809 canonical keys")
    return groups, sources, counts


def build(args):
    root, output = args.release_root.resolve(), args.output.resolve()
    head = git_head(root)
    if args.release_commit and head != args.release_commit:
        raise Refused(f"release head {head} differs from requested {args.release_commit}")
    groups, sources, counts = prepare(args.templates, args.teach, args.instruction)
    if output.exists():
        raise Refused(f"output already exists: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    temp = Path(tempfile.mkdtemp(prefix=output.name + ".", dir=output.parent))
    try:
        files = []
        for name, rows in zip(ORDER, groups):
            target = temp / name
            target.write_text(json.dumps(rows, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
            files.append({"path": name, "rows": len(rows), "sha256": digest_bytes(target.read_bytes())})
        core = {"version": 2, "release_commit": head, "course": "foundations",
                "status": "pending-ai-review", "ai_approval": "pending",
                "import_order": files, "counts": counts, "canonical_kps": 809,
                "sources": sources, "side_effects": {"production_writes": 0, "approvals": 0}}
        receipt = core | {"bundle_sha256": digest_json(core)}
        (temp / "bundle.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
        temp.rename(output)
    except Exception:
        shutil.rmtree(temp, ignore_errors=True)
        raise
    print(f"FOUNDATIONS RELEASE BUNDLE OK: {output} {receipt['bundle_sha256']}")


def validate_receipt(receipt,root):
    if not isinstance(receipt, dict) or receipt.get("version") not in (1, 2):
        raise Refused("unsupported or missing bundle receipt")
    core = {key: value for key, value in receipt.items() if key != "bundle_sha256"}
    if receipt.get("bundle_sha256") != digest_json(core):
        raise Refused("bundle receipt fingerprint mismatch")
    if receipt.get("release_commit") != git_head(root):
        raise Refused("bundle release commit differs from the checkout head")
    if receipt.get("counts") != EXPECTED or receipt.get("canonical_kps") != 809:
        raise Refused("bundle count contract differs from 809/809/809")
    status, approval = ("pending-human-review", "human_approval") if receipt["version"] == 1 else ("pending-ai-review", "ai_approval")
    if (receipt.get("status") != status
            or receipt.get(approval) != "pending"
            or receipt.get("side_effects") != {"production_writes": 0, "approvals": 0}):
        raise Refused("bundle lifecycle boundary changed")


def read_bundle_rows(directory,files):
    rows = []
    for item in files:
        path = directory / item["path"]
        if digest_bytes(path.read_bytes()) != item.get("sha256"):
            raise Refused(f"bundle member changed: {path}")
        part = read_json(path)
        if not isinstance(part, list) or len(part) != item.get("rows"):
            raise Refused(f"bundle member count changed: {path}")
        rows.extend(part)
    return rows


def validate_bundle_contract(rows):
    validate_rows(rows, "bundle", set(EXPECTED))
    pairs = {(row["kp_id"], row["kind"]) for row in rows}
    if len(rows) != 2427 or len(pairs) != 2427:
        raise Refused("bundle must contain 2,427 unique pending documents")
    counts = {kind: sum(row["kind"] == kind for row in rows) for kind in EXPECTED}
    keysets = {kind: {row["kp_id"] for row in rows if row["kind"] == kind}
               for kind in EXPECTED}
    if counts != EXPECTED or len(set(map(frozenset, keysets.values()))) != 1:
        raise Refused("bundle rows differ from the 809/809/809 canonical contract")


def verify(args):
    directory, root = args.bundle.resolve(), args.release_root.resolve()
    receipt = read_json(directory / "bundle.json")
    validate_receipt(receipt,root)
    files = receipt.get("import_order")
    if not isinstance(files, list) or [item.get("path") for item in files] != list(ORDER):
        raise Refused("bundle import order changed")
    validate_bundle_contract(read_bundle_rows(directory,files))
    print(f"FOUNDATIONS RELEASE BUNDLE VERIFIED: {receipt['bundle_sha256']}")


def verify_recovery(args):
    receipt = read_json(args.receipt.resolve())
    head = git_head(args.release_root.resolve())
    archive = receipt.get("archive") if isinstance(receipt, dict) else None
    fingerprints = [receipt.get(name) for name in (
        "production_read_only_fingerprint", "restored_fingerprint_before",
        "restored_fingerprint_after")]
    valid_archive = (isinstance(archive, dict) and archive.get("encrypted") is True
                     and archive.get("plaintext_dump_written") is False
                     and isinstance(archive.get("sha256"), str)
                     and len(archive["sha256"]) == 64)
    if receipt.get("candidate_head") != head:
        raise Refused("recovery receipt belongs to another release commit")
    if not valid_archive or receipt.get("disposable_database_removed") is not True:
        raise Refused("recovery receipt lacks encrypted backup/restore evidence")
    archive_path = Path(archive.get("path", ""))
    if not archive_path.is_file() or digest_bytes(archive_path.read_bytes()) != archive["sha256"]:
        raise Refused("encrypted backup is missing or differs from its receipt")
    if (receipt.get("outbound_model_keys") != "empty"
            or any(not isinstance(value, str) or not value for value in fingerprints)
            or len(set(fingerprints)) != 1):
        raise Refused("recovery replay was not isolated or byte-identical")
    print(f"ENCRYPTED BACKUP AND RESTORE VERIFIED: {args.receipt.resolve()}")


def parser():
    command = argparse.ArgumentParser(description=__doc__)
    subs = command.add_subparsers(dest="command", required=True)
    make = subs.add_parser("build")
    make.add_argument("--release-root", required=True, type=Path)
    make.add_argument("--release-commit")
    make.add_argument("--templates", required=True, type=Path)
    make.add_argument("--teach", required=True, type=Path)
    make.add_argument("--instruction", required=True, type=Path)
    make.add_argument("--output", required=True, type=Path)
    check = subs.add_parser("verify")
    check.add_argument("--release-root", required=True, type=Path)
    check.add_argument("--bundle", required=True, type=Path)
    recovery = subs.add_parser("verify-recovery")
    recovery.add_argument("--release-root", required=True, type=Path)
    recovery.add_argument("--receipt", required=True, type=Path)
    return command


def main():
    args = parser().parse_args()
    try:
        if args.command == "build":
            build(args)
        elif args.command == "verify":
            verify(args)
        else:
            verify_recovery(args)
    except (Refused, OSError, subprocess.CalledProcessError) as error:
        print(f"foundations release bundle refused: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
