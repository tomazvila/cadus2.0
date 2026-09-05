#!/usr/bin/env bash
# Part of scripts/check_ops.sh: check (j) caddy: the file, the mount, and `caddy validate`.
#
# scripts/check_ops.sh sources this file and calls its functions in order. The
# functions read and write the global variables of that script: `rc`, the
# image lists, and the plan. Run scripts/check_ops.sh, not this file.

# ---------------------------------------------------------------------------
# (j) the edge serves the SPA and the API on ONE origin, with the service's own
#     security headers
#
# `crates/web/src/origin.rs` answers `403 cross_origin_rejected` to a
# cookie-carrying write whose `Origin` is not this deployment's own. So the
# bundle and /api MUST come off one origin, and `deploy/Caddyfile` is the only
# file that says so. Nothing else in the gate reads it: it is a bind mount, so a
# broken routing block passes every image check above and fails first on the
# operator's server.
#
# The check reads three files and holds four facts:
#
#   1. /api/* reaches the web service. The `@api` handle proxies to `web:8080`.
#   2. Every other path is the SPA: the fallback `handle` has a `root`, a
#      `file_server`, and the `try_files {path} /index.html` line that makes
#      `/ops` and `/review` reload instead of answer 404.
#   3. That `root` is the path the Dockerfile copies the bundle to. Move the COPY
#      destination alone and Caddy serves an empty directory.
#   4. The five headers of the fallback `handle` equal SECURITY_HEADERS of
#      `crates/web/src/security.rs`, name and value, character for character. The
#      service stamps them on its OWN answers only. The document is a file Caddy
#      serves, and a Content-Security-Policy that never reaches the document
#      protects nothing.
# ---------------------------------------------------------------------------
check_caddy_file() {
caddy_ok=1
caddy_log=""
if caddy_log="$(python3 - <<'PYCADDY'
import io
import re

CADDYFILE = "deploy/Caddyfile"
SECURITY = "crates/web/src/security.rs"
DOCKERFILE = "Dockerfile"

problems = []


def records(text):
    """Every non-blank, non-comment line, with the brace depth it opens at."""
    out = []
    depth = 0
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        out.append((depth, line))
        depth += line.count("{") - line.count("}")
    return out


def body_of(rows, start):
    """The rows inside the block that rows[start] opens."""
    floor = rows[start][0]
    out = []
    for depth, line in rows[start + 1:]:
        if depth <= floor:
            break
        out.append((depth, line))
    return out


def header_pairs(rows):
    """The `header { Name value }` block of a handle body, as (name, value)."""
    for index, (_, line) in enumerate(rows):
        if line != "header {":
            continue
        pairs = []
        for _, entry in body_of(rows, index):
            if entry == "}":
                continue
            name, _, value = entry.partition(" ")
            pairs.append((name, value.strip().strip('"')))
        return pairs
    return None


def rust_string(literal):
    """A Rust string literal, with its backslash line continuations folded away."""
    body = literal.strip()
    if not (body.startswith('"') and body.endswith('"')):
        return None
    return re.sub(r"\\\n\s*", "", body[1:-1])


caddy = io.open(CADDYFILE, encoding="utf-8").read()
rows = records(caddy)

# --- 1 and 2: the two handles ----------------------------------------------
api_body = []
ops_body = []
fallback_body = []
for index, (depth, line) in enumerate(rows):
    if depth != 1 or not line.startswith("handle"):
        continue
    matcher = line[len("handle"):].strip().rstrip("{").strip()
    if matcher == "@api":
        api_body = body_of(rows, index)
    elif matcher == "@ops":
        ops_body = body_of(rows, index)
    elif matcher == "":
        fallback_body = body_of(rows, index)

api_matcher = [line for _, line in rows if line.startswith("@api ")]
if not api_matcher:
    problems.append(CADDYFILE + " declares no @api matcher")
elif api_matcher[0] != "@api path /api/*":
    problems.append(
        CADDYFILE + " matches the API as " + repr(api_matcher[0])
        + "; the service prefix is /api/ (crates/web/src/origin.rs API_PREFIX)"
    )

if not api_body:
    problems.append(CADDYFILE + " has no `handle @api` block, so /api reaches no service")
elif not any(
    line in ("reverse_proxy web:8080 {", "reverse_proxy web:8080") for _, line in api_body
):
    problems.append(CADDYFILE + " does not proxy the @api handle to web:8080")

if not fallback_body:
    problems.append(CADDYFILE + " has no matcher-less `handle` block, so no path serves the SPA")

# --- the @ops guard ---------------------------------------------------------
# /api/ready reports the datastore verdict and the age of the diagnosis backlog
# (D-M5-6), and /metrics reports every request series. Both are for the compose
# network. Delete the matcher or the 404 and the edge publishes them to every
# visitor (M6 review, finding F23).
OPS_PATHS = ("/api/ready", "/metrics")
ops_matcher = [line for _, line in rows if line.startswith("@ops ")]
if not ops_matcher:
    problems.append(
        CADDYFILE + " declares no @ops matcher, so " + " and ".join(OPS_PATHS)
        + " answer through the edge"
    )
elif not ops_matcher[0].startswith("@ops path "):
    problems.append(
        CADDYFILE + " declares the ops matcher as " + repr(ops_matcher[0])
        + "; it matches on `path`"
    )
else:
    guarded = ops_matcher[0].split()[2:]
    for path in OPS_PATHS:
        if path not in guarded:
            problems.append(
                CADDYFILE + " does not guard " + path + " in the @ops matcher, so the"
                " edge publishes it"
            )

if not ops_body:
    problems.append(
        CADDYFILE + " has no `handle @ops` block, so the @ops paths fall through to the"
        " SPA handle and answer 200"
    )
elif not any(line.startswith("respond 404") for _, line in ops_body):
    problems.append(CADDYFILE + " does not answer 404 in the `handle @ops` block")

roots = [line for _, line in fallback_body if line.startswith("root ")]
if not roots:
    problems.append(CADDYFILE + " sets no `root` in the SPA handle, so Caddy serves no bundle")
if not any(line == "file_server" for _, line in fallback_body):
    problems.append(CADDYFILE + " runs no `file_server` in the SPA handle")
if not any(line == "try_files {path} /index.html" for _, line in fallback_body):
    problems.append(
        CADDYFILE + " has no `try_files {path} /index.html` in the SPA handle; /ops and"
        " /review then answer 404 on a reload"
    )

# --- 3: the root is the Dockerfile's copy destination -----------------------
dockerfile = io.open(DOCKERFILE, encoding="utf-8").read()
copies = re.findall(r"^COPY --from=spa-builder \S+ (\S+)\s*$", dockerfile, re.M)
if not copies:
    problems.append(DOCKERFILE + " has no `COPY --from=spa-builder ... <root>` line")
elif roots:
    served = roots[0].split()[-1]
    if served != copies[0]:
        problems.append(
            CADDYFILE + " serves " + served + " and " + DOCKERFILE
            + " copies the bundle to " + copies[0]
        )

# --- 4: the five headers match the service's own ----------------------------
rust = io.open(SECURITY, encoding="utf-8").read()
csp_match = re.search(r"pub const CONTENT_SECURITY_POLICY: &str =(.*?);\n", rust, re.S)
array = re.search(
    r"pub const SECURITY_HEADERS: \[\(&str, &str\); (\d+)\] = \[(.*?)\n\];", rust, re.S
)
csp = rust_string(csp_match.group(1)) if csp_match else None

if csp is None or array is None:
    problems.append(
        "could not read CONTENT_SECURITY_POLICY and SECURITY_HEADERS from " + SECURITY
    )
else:
    wanted = []
    for name, value in re.findall(
        r'\(\s*"([^"]+)"\s*,\s*(CONTENT_SECURITY_POLICY|"[^"]*")\s*\)', array.group(2)
    ):
        wanted.append(
            (name.lower(), csp if value == "CONTENT_SECURITY_POLICY" else value[1:-1])
        )
    if len(wanted) != int(array.group(1)):
        problems.append(
            SECURITY + " declares " + array.group(1) + " headers and the reader found "
            + str(len(wanted))
        )
    served_pairs = header_pairs(fallback_body)
    if served_pairs is None:
        problems.append(
            CADDYFILE + " stamps no `header` block in the SPA handle, so the document ships"
            " with no Content-Security-Policy"
        )
    else:
        served_map = {name.lower(): value for name, value in served_pairs}
        for name, value in wanted:
            if name not in served_map:
                problems.append(CADDYFILE + " does not stamp " + name + " on the SPA")
            elif served_map[name] != value:
                problems.append(
                    CADDYFILE + " stamps " + name + " as " + repr(served_map[name])
                    + " and " + SECURITY + " sends " + repr(value)
                )
        for name in served_map:
            if name not in {entry[0] for entry in wanted}:
                problems.append(
                    CADDYFILE + " stamps " + name + ", which " + SECURITY + " does not send"
                )

for line in problems:
    print(line)
PYCADDY
)"; then
    if [ -n "$caddy_log" ]; then
        printf 'FAIL: caddy    -- %s\n' "$caddy_log"
        caddy_ok=0
        rc=1
    fi
else
    echo "FAIL: caddy    -- the Caddyfile check did not run"
    printf '%s\n' "$caddy_log"
    caddy_ok=0
    rc=1
fi
}

# --- 5: the compose file mounts the file this check just read ---------------
#
# Every fact above is a fact about `deploy/Caddyfile` in the repository. The
# `spa` image copies the bundle and NO Caddyfile, so the container runs the
# stock `caddy:2` configuration unless the compose file bind-mounts this file
# over /etc/caddy/Caddyfile. Delete that one line and the deployment serves the
# Caddy welcome page while every check above stays green (M6 review, finding
# F23).
#
# The bind reaches that path in one of two shapes, and the reader below accepts
# both:
#
#   the FILE      -- `./deploy/Caddyfile:/etc/caddy/Caddyfile`
#   the DIRECTORY -- `./deploy:/etc/caddy`, and Caddy reads the Caddyfile inside
#
# The two are NOT the same at run time. Check (l) below drives that difference
# against the real edge image and fails the file shape (M6 review 2, finding
# V9). This reader holds the shape-free fact alone: some bind of this
# repository's `deploy/Caddyfile` lands at /etc/caddy/Caddyfile. It also prints
# one `MOUNT <source relative to the repository root> <container target>` line,
# which check (l) replays in its sandbox, so the probe reads the compose file
# and never a second copy of the mount.
check_caddy_mount() {
mount_log=""
mount_spec=""
mount_read=""
if mount_read="$(printf '%s' "$config_json" | python3 -c '
import json
import posixpath
import sys

TARGET = "/etc/caddy/Caddyfile"
SOURCE = "deploy/Caddyfile"

root = sys.argv[1].replace("\\", "/").rstrip("/") + "/"
doc = json.load(sys.stdin)
problems = []
spec = ""
edge = []

for name, service in sorted(doc.get("services", {}).items()):
    build = service.get("build") or {}
    if build.get("target") == "spa":
        edge.append((name, service))

if not edge:
    problems.append("no service builds the spa image, so nothing serves the bundle")

for name, service in edge:
    # The bind that carries TARGET: the file itself, or a directory above it.
    carrier = None
    for volume in service.get("volumes", []) or []:
        if not isinstance(volume, dict):
            continue
        target = str(volume.get("target", "")).replace("\\", "/")
        if target == TARGET or TARGET.startswith(target.rstrip("/") + "/"):
            carrier = (volume, target.rstrip("/") or "/")
            break
    if carrier is None:
        problems.append(
            "the " + name + " service mounts nothing at " + TARGET
            + "; the container then runs the stock caddy:2 configuration and serves"
            " the Caddy welcome page"
        )
        continue
    volume, target = carrier
    source = str(volume.get("source", "")).replace("\\", "/")
    # The repository path that reaches TARGET through this bind. A file bind
    # reaches it directly; a directory bind reaches it through the rest of the
    # container path.
    inside = TARGET[len(target):].lstrip("/")
    reached = posixpath.normpath(posixpath.join(source, inside)) if inside else source
    if not reached.endswith(SOURCE):
        problems.append(
            "the " + name + " service mounts " + repr(source) + " at " + target
            + ", so " + TARGET + " reads " + repr(reached) + " and not " + SOURCE
            + ", which is the file this check reads"
        )
        continue
    relative = source[len(root):] if source.startswith(root) else source
    spec = relative + " " + target

for line in problems:
    print(line)
if spec:
    print("MOUNT " + spec)
' "$repo_root")"; then
    mount_log="$(printf '%s\n' "$mount_read" | grep -v '^MOUNT ' || true)"
    mount_spec="$(printf '%s\n' "$mount_read" | sed -n 's/^MOUNT //p' | head -n 1)"
    if [ -n "$mount_log" ]; then
        printf 'FAIL: caddy    -- %s\n' "$mount_log"
        caddy_ok=0
        rc=1
    fi
else
    echo "FAIL: caddy    -- the Caddyfile mount check did not run"
    caddy_ok=0
    rc=1
fi
}

# --- 6: Caddy itself accepts the file ---------------------------------------
#
# The reader above holds no grammar of the Caddyfile: it counts braces and reads
# lines. A file that Caddy REFUSES therefore passed the whole ops gate, and the
# operator met the fault as a restart loop on the sole ingress (M6 review,
# finding F13). `caddy validate` in the edge image is the authority: it runs the
# same Caddy build that the deployment runs, and it reads the same file through
# the same path. SITE_ADDRESS stands in for `.env`, because the file names it.
check_caddy_validate() {
validate_ok=1
while read -r image; do
    [ -n "$image" ] || continue
    validate_log=""
    if ! validate_log="$(docker run --rm --entrypoint caddy \
        -e SITE_ADDRESS="$SITE_ADDRESS" \
        -v "$repo_root/deploy/Caddyfile:/etc/caddy/Caddyfile:ro" \
        "$image" validate --config /etc/caddy/Caddyfile --adapter caddyfile 2>&1)"; then
        echo "FAIL: caddy    -- caddy validate refuses deploy/Caddyfile in image $image"
        printf '%s\n' "$validate_log"
        validate_ok=0
        caddy_ok=0
        rc=1
    fi
done <<EOF
$spa_images
EOF

if [ "$caddy_ok" -eq 1 ] && [ "$validate_ok" -eq 1 ]; then
    echo "PASS: caddy    -- caddy validate accepts deploy/Caddyfile in the edge image, the compose file mounts it at /etc/caddy/Caddyfile, /api proxies to web:8080, @ops answers 404 for /api/ready and /metrics, the SPA falls back to index.html under the Dockerfile's root, and the five security headers match crates/web/src/security.rs"
fi
}

