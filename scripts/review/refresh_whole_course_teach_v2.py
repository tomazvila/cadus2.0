#!/usr/bin/env python3
"""Refresh pending whole-course Teach technical evidence without semantic approval."""
import argparse
import hashlib
import json
import os
import tempfile
from pathlib import Path

PARTS = tuple(range(1, 31))
COUNT = 735


class Refused(ValueError):
    """Evidence does not bind the checkout that would be updated."""


def sha256_bytes(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise Refused(f"{path}: {error}") from error


def part_paths(kind):
    return [f"{kind}/part-{part:02}.json" for part in PARTS]


def raw_hashes(root, paths):
    hashes = {}
    for relative in paths:
        path = root / relative
        if not path.is_file():
            raise Refused(f"missing bound input: {relative}")
        hashes[relative] = sha256_bytes(path.read_bytes())
    return hashes


def curriculum_hashes(root):
    curriculum = root / "curriculum"
    paths = sorted(
        path.relative_to(root).as_posix()
        for path in curriculum.rglob("*")
        if path.is_file() and path.suffix in {".yaml", ".yml"}
    )
    if not paths:
        raise Refused("no curriculum YAML inputs")
    return raw_hashes(root, paths)


def canonical_sha(value):
    data = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(data).hexdigest()


def keyed(rows, label):
    result = {}
    for row in rows:
        key = row.get("kp_id") if isinstance(row, dict) else None
        if not isinstance(key, str) or not key or key in result:
            raise Refused(f"{label}: invalid or duplicate kp_id")
        result[key] = row
    return result


def validate_technical_row(key, row):
    fields = {"kp_id", "teach_digest", "template_digest", "collision", "production_gate", "context_coverage", "ai_review"}
    if set(row) != fields or row["kp_id"] != key:
        raise Refused(f"technical row shape: {key}")
    if row["collision"] != "clear" or row["production_gate"] != "accepted":
        raise Refused(f"technical gate: {key}")
    if row["context_coverage"] != "sampled_template_instances" or row["ai_review"] != "pending":
        raise Refused(f"technical lifecycle: {key}")
    if not all(isinstance(row[field], str) and row[field] for field in ("teach_digest", "template_digest")):
        raise Refused(f"technical digests: {key}")


def evidence_rows(evidence):
    if not isinstance(evidence, dict) or evidence.get("schema_version") != 2:
        raise Refused("unsupported technical evidence schema")
    if evidence.get("status") != "pending-ai-review":
        raise Refused("technical evidence is not pending AI review")
    source_paths = ["inputs/templates.json", *part_paths("drafts")]
    rows = evidence.get("rows")
    if not isinstance(rows, list) or len(rows) != COUNT:
        raise Refused("technical evidence must contain 735 rows")
    technical = keyed(rows, "technical evidence")
    for key, row in technical.items():
        validate_technical_row(key, row)
    bindings = evidence.get("historical_row_sha256")
    if not isinstance(bindings, dict) or set(bindings) != set(technical):
        raise Refused("historical row binding keys differ")
    if not all(isinstance(value, str) and value.startswith("sha256:") for value in bindings.values()):
        raise Refused("invalid historical row binding")
    return source_paths, technical, bindings



def historical_review_rows(sidecar, index):
    historical = {}
    review_files = index.get("reviews")
    if not isinstance(review_files, list) or {row.get("path") for row in review_files if isinstance(row, dict)} != set(part_paths("reviews")):
        raise Refused("historical review inventory")
    for entry in review_files:
        relative = entry["path"]
        path = sidecar / "historical-archive" / relative
        if entry.get("sha256") != sha256_bytes(path.read_bytes()):
            raise Refused(f"historical review changed: {relative}")
        for row in read_json(path):
            key = row.get("kp_id")
            digest = sha256_bytes(json.dumps(row, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode())
            if not isinstance(key, str) or key in historical:
                raise Refused("invalid or duplicate historical KP")
            historical[key] = (f"historical-archive/{relative}", digest)
    return historical


def archive_rows(sidecar, manifest, reviews, bindings):
    archive = manifest.get("historical_archive")
    if not isinstance(archive, dict) or archive.get("path") != "historical-archive/index.json":
        raise Refused("missing historical archive binding")
    index_path = sidecar / archive["path"]
    if archive.get("sha256") != sha256_bytes(index_path.read_bytes()):
        raise Refused("historical archive index changed")
    index = read_json(index_path)
    if index.get("status") != "historical-as-encountered":
        raise Refused("historical archive status")
    expected = {"import-manifest.v1.json", "manifest.v1.json", *part_paths("reviews")}
    files = index.get("files")
    if not isinstance(files, list) or {row.get("path") for row in files if isinstance(row, dict)} != expected:
        raise Refused("historical archive inventory")
    if len(files) != len(expected):
        raise Refused("duplicate historical archive path")
    for row in files:
        path = row["path"]
        if row.get("sha256") != sha256_bytes((sidecar / "historical-archive" / path).read_bytes()):
            raise Refused(f"historical archive file changed: {path}")
    historical = historical_review_rows(sidecar, index)
    if set(historical) != set(bindings) or any(bindings[key] != value[1] for key, value in historical.items()):
        raise Refused("historical row bindings changed")
    for row in reviews:
        reference = row.get("historical_review") if isinstance(row, dict) else None
        expected_path, expected_sha = historical.get(row.get("kp_id"), (None, None))
        if reference != {"path": expected_path, "sha256": expected_sha}:
            raise Refused("current historical reference changed")


def inventory(sidecar):
    paths = ["inputs/coverage.json", "inputs/templates.json", *part_paths("drafts"), *part_paths("reviews")]
    result = []
    for relative in paths:
        value = read_json(sidecar / relative)
        rows = value if isinstance(value, list) else value.get("rows")
        if not isinstance(rows, list):
            raise Refused(f"inventory rows: {relative}")
        result.append({"path": relative, "rows": len(rows), "sha256": hashlib.sha256((sidecar / relative).read_bytes()).hexdigest()})
    return result


def validate(root, evidence):
    sidecar = root / "docs/content-foundations/whole-course-teach"
    if not sidecar.is_dir() or not (sidecar / "historical-archive/index.json").is_file():
        raise Refused("missing whole-course sidecar or historical archive")
    source_paths, technical, bindings = evidence_rows(evidence)
    if evidence.get("raw_input_sha256") != raw_hashes(sidecar, source_paths):
        raise Refused("stale document inputs")
    if evidence.get("curriculum_raw_input_sha256") != curriculum_hashes(root):
        raise Refused("stale curriculum inputs")
    drafts, reviews = [], []
    for path in part_paths("drafts"):
        rows = read_json(sidecar / path)
        if not isinstance(rows, list):
            raise Refused(f"draft shard is not an array: {path}")
        drafts.extend(rows)
    for path in part_paths("reviews"):
        rows = read_json(sidecar / path)
        if not isinstance(rows, list):
            raise Refused(f"review shard is not an array: {path}")
        reviews.extend(rows)
    if set(keyed(drafts, "drafts")) != set(technical) or len(drafts) != COUNT:
        raise Refused("draft keys differ from technical evidence")
    if set(keyed(reviews, "reviews")) != set(technical) or len(reviews) != COUNT:
        raise Refused("review keys differ from technical evidence")
    manifest = read_json(sidecar / "manifest.json")
    if manifest.get("schema_version") != 2 or manifest.get("status") != "pending-ai-review":
        raise Refused("current manifest lifecycle differs")
    archive_rows(sidecar, manifest, reviews, bindings)
    return sidecar, manifest, technical, bindings


def refreshed_review(old, technical, binding):
    if not isinstance(old, dict) or old.get("historical_review", {}).get("sha256") != binding:
        raise Refused(f"historical binding mismatch: {technical['kp_id']}")
    historical = old["historical_review"]
    if set(historical) != {"path", "sha256"} or not isinstance(historical["path"], str):
        raise Refused(f"historical reference shape: {technical['kp_id']}")
    return {"kp_id": technical["kp_id"], "teach_digest": technical["teach_digest"],
            "template_digest": technical["template_digest"],
            "verification": {field: technical[field] for field in ("collision", "production_gate", "context_coverage")},
            "ai_review": "pending", "historical_review": historical}


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            json.dump(value, handle, ensure_ascii=False, indent=2)
            handle.write("\n")
        os.replace(temporary, path)
    except BaseException:
        Path(temporary).unlink(missing_ok=True)
        raise


def update(root, evidence_path):
    evidence = read_json(evidence_path)
    sidecar, manifest, technical, bindings = validate(root, evidence)
    updates, all_reviews = {}, []
    for relative in part_paths("reviews"):
        old_rows = read_json(sidecar / relative)
        updates[relative] = [refreshed_review(row, technical[row["kp_id"]], bindings[row["kp_id"]]) for row in old_rows]
        all_reviews.extend(updates[relative])
    refreshed_manifest = dict(manifest)
    evidence_raw = json.dumps(evidence, ensure_ascii=False, indent=2).encode() + b"\n"
    # Build all content outputs before touching the sidecar, then hash the exact staged bytes.
    staged = {relative: json.dumps(rows, ensure_ascii=False, indent=2).encode() + b"\n" for relative, rows in updates.items()}
    staged["technical-evidence-v2.json"] = evidence_raw
    files = []
    for item in inventory(sidecar):
        relative = item["path"]
        if relative in staged:
            value = json.loads(staged[relative])
            item["rows"] = len(value)
            item["sha256"] = hashlib.sha256(staged[relative]).hexdigest()
        files.append(item)
    refreshed_manifest["files"] = files
    refreshed_manifest["technical_evidence"] = {"path": "technical-evidence-v2.json", "sha256": sha256_bytes(evidence_raw)}
    drafts = [row for relative in part_paths("drafts") for row in read_json(sidecar / relative)]
    arrays = dict(manifest.get("canonical_arrays", {}))
    arrays["drafts_sha256"], arrays["reviews_sha256"] = canonical_sha(drafts), canonical_sha(all_reviews)
    refreshed_manifest["canonical_arrays"] = arrays
    atomic_json(sidecar / "technical-evidence-v2.json", evidence)
    for relative, rows in updates.items():
        atomic_json(sidecar / relative, rows)
    atomic_json(sidecar / "manifest.json", refreshed_manifest)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--update", action="store_true")
    args = parser.parse_args()
    if not args.update:
        parser.error("--update is required; validation-only runs do not write")
    update(args.root.resolve(), args.evidence.resolve())


if __name__ == "__main__":
    main()
