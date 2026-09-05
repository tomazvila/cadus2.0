#!/usr/bin/env bash
# Part of scripts/check_ops.sh: checks (f) invariants, (g) shell, (h) ci, and (i) bench.
#
# scripts/check_ops.sh sources this file and calls its functions in order. The
# functions read and write the global variables of that script: `rc`, the
# image lists, and the plan. Run scripts/check_ops.sh, not this file.

# ---------------------------------------------------------------------------
# (f) the compose invariants of the review rounds
#
# The seven A4 model variables reach the `worker` service, because model calls
# run in cadus-worker and never on a request path (R4, L6). Compose keeps a key
# whose `${VAR:-}` value is unset, and gives it an empty string, so the check
# reads the KEY and never the value: an empty OPENAI_API_KEY is the documented
# "call no model" deployment (docs/SELF_HOST.md).
# ---------------------------------------------------------------------------
check_invariants() {
invariant_log=""
if invariant_log="$(printf '%s' "$config_json" | python3 -c '
import json
import sys

doc = json.load(sys.stdin)
services = doc.get("services", {})
problems = []

db = services.get("db", {})
test = db.get("healthcheck", {}).get("test", [])
if isinstance(test, str):
    test = [test]
probe = " ".join(str(part) for part in test)
if "pg_isready" not in probe:
    problems.append("the db healthcheck does not run pg_isready: " + probe)
elif " -h " not in probe:
    problems.append("the db healthcheck does not probe TCP (no -h): " + probe)

migrate_env = services.get("migrate", {}).get("environment", {}) or {}
for key in ("DB_STATEMENT_TIMEOUT_MS", "DB_CLIENT_TIMEOUT_MS"):
    if key in migrate_env:
        problems.append("migrate carries " + key + "; a migration runs unbounded")

# The seven A4 variables of docs/SELF_HOST.md, section "The seven variables".
model_keys = (
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_MODEL",
    "OPENROUTER_PROVIDER_ORDER",
    "DIAGNOSIS_OUTPUT_TOKENS",
    "DIAGNOSIS_REASONING_MAX_TOKENS",
    "DIAGNOSIS_CALLS_PER_SESSION",
)
worker_env = services.get("worker", {}).get("environment", {}) or {}
missing = [key for key in model_keys if key not in worker_env]
if missing:
    problems.append(
        "the worker service does not carry the A4 model variable(s) "
        + ", ".join(missing)
        + "; the diagnosis job then reads an empty OPENAI_API_KEY and calls no model"
    )

for line in problems:
    print(line)
')"; then
    if [ -n "$invariant_log" ]; then
        printf 'FAIL: invariants -- %s\n' "$invariant_log"
        rc=1
    else
        echo "PASS: invariants -- db probes TCP, migrate carries no query bound, and worker carries the seven A4 model variables"
    fi
else
    echo "FAIL: invariants -- the compose invariant check did not run"
    rc=1
fi
}

# ---------------------------------------------------------------------------
# (g) the shell scripts pass shellcheck
#
# `-S warning` is the gate level: it reports error and warning and holds back
# style and info. Fix a warning; do not silence it. A `# shellcheck disable=`
# line needs a comment above it that says why the rule does not apply here.
# ---------------------------------------------------------------------------
check_shell() {
shell_log=""
if shell_log="$(shellcheck -S warning -x scripts/*.sh 2>&1)"; then
    echo "PASS: shell    -- shellcheck -S warning reports nothing on scripts/*.sh"
else
    echo "FAIL: shell    -- shellcheck -S warning reports a finding on scripts/*.sh"
    printf '%s\n' "$shell_log"
    rc=1
fi
}

# ---------------------------------------------------------------------------
# (h) the CI workflow publishes every service port on the loopback interface
#
# The Postgres service of the gate job runs with POSTGRES_HOST_AUTH_METHOD=trust
# (`.github/workflows/ci.yml` says why). A `ports:` entry of `5432:5432` binds
# every interface of the runner, so any process that reaches the runner over the
# network connects to that database as the superuser. `127.0.0.1:5432:5432`
# keeps the port on the runner itself, and the gate steps run there.
#
# The reader below is a line scan, not a YAML parser: the runner image and this
# box carry no PyYAML.
# ---------------------------------------------------------------------------
check_ci() {
workflow="ci.yml"
ci_log=""
if [ ! -f ".github/workflows/$workflow" ]; then
    echo "FAIL: ci       -- .github/workflows/$workflow is missing"
    rc=1
elif ci_log="$(python3 -c '
import io
import sys

path = sys.argv[1]
problems = []
in_ports = False
ports_indent = 0
entries = 0

for number, raw in enumerate(io.open(path, encoding="utf-8"), start=1):
    line = raw.rstrip("\n")
    stripped = line.strip()
    if not stripped or stripped.startswith("#"):
        continue
    indent = len(line) - len(line.lstrip())
    if stripped == "ports:":
        in_ports = True
        ports_indent = indent
        continue
    if not in_ports:
        continue
    if not stripped.startswith("-") or indent <= ports_indent:
        in_ports = False
        continue
    value = stripped[1:].strip().strip("\"'"'"'")
    entries += 1
    if not value.startswith("127.0.0.1:"):
        problems.append("line %d publishes %s on every interface" % (number, value))

if entries == 0:
    problems.append("the workflow publishes no port; the reader found no ports: entry")

for problem in problems:
    print(problem)
' ".github/workflows/$workflow" 2>&1)"; then
    if [ -n "$ci_log" ]; then
        printf 'FAIL: ci       -- %s\n' "$ci_log"
        rc=1
    else
        echo "PASS: ci       -- every published port of $workflow binds 127.0.0.1"
    fi
else
    echo "FAIL: ci       -- the workflow port check did not run"
    printf '%s\n' "$ci_log"
    rc=1
fi
}

# ---------------------------------------------------------------------------
# (i) the budget benchmarks run AFTER the test suite and never beside it
#
# Spec section 10.5 and docs/reference/l1-budget.md section 7: parallel suites
# contend on one box and on a two-core runner, and a contended benchmark
# measures the scheduler and not the code. The rule has two halves, and this
# check reads both:
#
#   1. scripts/gate.sh runs `cargo test --workspace` BEFORE scripts/bench.sh.
#      The gate is one shell script, so the order of the two lines is the order
#      of the two steps.
#   2. .github/workflows/ci.yml declares ONE job. A second job runs beside the
#      gate on its own runner, and a benchmark job among them measures a shared
#      machine. One `runs-on:` line is one job.
#
# M5 U12 added the check. Before it, nothing failed a workflow edit that moved
# the benchmarks into a job of their own.
# ---------------------------------------------------------------------------
check_bench() {
bench_rc=0
if [ ! -f scripts/gate.sh ]; then
    echo "FAIL: bench    -- scripts/gate.sh is missing"
    bench_rc=1
else
    # The COMMAND lines, not the `echo` lines that announce them: a moved
    # command under an unmoved banner must fail this check.
    test_line="$(grep -n '^cargo test --workspace$' scripts/gate.sh | head -1 | cut -d: -f1)"
    bench_line="$(grep -n '^ *\(bash \)\?scripts/bench.sh$' scripts/gate.sh | tail -1 | cut -d: -f1)"
    if [ -z "$test_line" ]; then
        echo "FAIL: bench    -- scripts/gate.sh runs no 'cargo test --workspace'"
        bench_rc=1
    elif [ -z "$bench_line" ]; then
        echo "FAIL: bench    -- scripts/gate.sh runs no scripts/bench.sh"
        bench_rc=1
    elif [ "$test_line" -ge "$bench_line" ]; then
        echo "FAIL: bench    -- scripts/gate.sh runs the benchmarks at line $bench_line, at or before the test suite at line $test_line"
        bench_rc=1
    fi
fi

if [ ! -f ".github/workflows/ci.yml" ]; then
    echo "FAIL: bench    -- .github/workflows/ci.yml is missing"
    bench_rc=1
else
    runners="$(grep -c 'runs-on:' .github/workflows/ci.yml || true)"
    if [ "$runners" != "1" ]; then
        echo "FAIL: bench    -- .github/workflows/ci.yml declares $runners jobs; the benchmarks run in the one gate job and never beside it"
        bench_rc=1
    fi
    if ! grep -q 'scripts/gate.sh' .github/workflows/ci.yml; then
        echo "FAIL: bench    -- .github/workflows/ci.yml runs no scripts/gate.sh"
        bench_rc=1
    fi
fi

if [ "$bench_rc" -eq 0 ]; then
    echo "PASS: bench    -- the budget benchmarks run after cargo test, in the one CI job"
else
    rc=1
fi
}

