"""Evidence adapter for the offline production worker template gate."""
import json
import hashlib
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def source_fingerprint(root, files):
    digest = hashlib.sha256()
    for path in sorted(files):
        digest.update(str(path.relative_to(root)).encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def gate_source_hash():
    files = [ROOT / name for name in (
        "Cargo.toml", "Cargo.lock", "crates/core/Cargo.toml", "crates/worker/Cargo.toml",
        "crates/worker/build.rs", "crates/worker/gate_fingerprint.rs")]
    for directory in ("crates/core/src", "crates/worker/src"):
        files.extend((ROOT / directory).rglob("*.rs"))
    return source_fingerprint(ROOT, files)


def curriculum_source_hash(curriculum):
    return source_fingerprint(curriculum, curriculum.rglob("*.yaml"))


class GateFactsError(ValueError):
    """Missing, malformed, or stale production-gate evidence."""


def fingerprint(document):
    return json.dumps(document, sort_keys=True, separators=(",", ":"))


def load_gate_facts(path, curriculum, templates):
    if path is not None:
        try:
            return json.loads(path.read_text())
        except (OSError, json.JSONDecodeError) as error:
            raise GateFactsError(f"cannot read template gate facts: {error}") from error
    local_cargo = Path.home() / ".cargo/bin/cargo"
    cargo = os.environ.get("CARGO") or (str(local_cargo) if local_cargo.is_file() else "cargo")
    command = [cargo, "run", "--quiet", "-p", "cadus-worker", "--bin",
               "content_template_gate", "--", str(curriculum)]
    documents = [row["document"] for rows in templates.values() for row in rows]
    try:
        result = subprocess.run(command, input=json.dumps(documents), capture_output=True,
                                text=True, check=False)
    except OSError as error:
        raise GateFactsError(f"cannot start template gate adapter: {error}") from error
    if result.returncode:
        raise GateFactsError(f"template gate adapter failed ({result.returncode}): {result.stderr.strip()}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise GateFactsError(f"template gate adapter emitted invalid JSON: {error}") from error


def gate_results(facts, gate_facts, curriculum):
    """Bind cached evidence to the exact curriculum and complete recipe document."""
    if gate_facts is None:
        return {}
    if not isinstance(gate_facts, dict) or gate_facts.get("schema_version") != 1:
        raise GateFactsError("template gate facts must carry schema_version 1")
    try:
        if gate_facts.get("gate_source_hash") != gate_source_hash():
            raise GateFactsError("template gate facts were compiled from different gate sources")
        if gate_facts.get("curriculum_source_hash") != curriculum_source_hash(curriculum):
            raise GateFactsError("template gate facts do not match the current curriculum sources")
    except OSError as error:
        raise GateFactsError(f"cannot fingerprint current audit sources: {error}") from error
    digest = facts.get("curriculum_hash")
    if not isinstance(digest, str) or not digest or gate_facts.get("curriculum_hash") != digest:
        raise GateFactsError("template gate facts do not match the curriculum hash")
    recipes = gate_facts.get("recipes")
    if not isinstance(recipes, list):
        raise GateFactsError("template gate facts must contain a recipes list")
    results = {}
    for row in recipes:
        if not isinstance(row, dict) or not isinstance(row.get("document"), dict):
            raise GateFactsError("template gate fact must contain a document")
        if not isinstance(row.get("accepted"), bool):
            raise GateFactsError("template gate accepted must be boolean")
        if not row["accepted"] and (not isinstance(row.get("reason"), str) or not row["reason"]):
            raise GateFactsError("declined template gate fact must contain a reason")
        key = fingerprint(row["document"])
        if key in results:
            raise GateFactsError("duplicate template gate fact")
        results[key] = row
    return results


def declined_templates(templates, results):
    failures = []
    for evidence in templates:
        result = results.get(fingerprint(evidence["document"]))
        if result is None or not result["accepted"]:
            reason = "production gate evidence missing or recipe changed"
            if result is not None:
                reason = result["reason"]
            failures.append({"source": evidence["source"], "reason": reason})
    return failures
