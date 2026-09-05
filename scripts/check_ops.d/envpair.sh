#!/usr/bin/env bash
# Part of scripts/check_ops.sh: check (k) envpair.
#
# scripts/check_ops.sh sources this file and calls its functions in order. The
# functions read and write the global variables of that script: `rc`, the
# image lists, and the plan. Run scripts/check_ops.sh, not this file.

# ---------------------------------------------------------------------------
# (k) .env.example pairs SITE_ADDRESS with CADUS_WEB_INSECURE_COOKIE
#
# The two keys are one decision. `crates/web/src/cookie.rs` writes the session
# cookie as `__Host-cadus_session; Secure` at the default, and a browser
# DISCARDS such a cookie on an http:// origin without a word: the login answers
# 200 and the next authed write answers 401. A copied .env.example that serves
# http (SITE_ADDRESS `:80`, or a `http://` origin) must therefore carry
# `CADUS_WEB_INSECURE_COOKIE=1`, and an https deployment must not (M6 review,
# finding F14).
#
# The check holds three facts about .env.example:
#
#   1. The two keys agree. An http SITE_ADDRESS needs an ACTIVE
#      CADUS_WEB_INSECURE_COOKIE=1 line; a domain needs the value 0, or no
#      active line at all.
#   2. Each of the two blocks names the other key, so an operator who edits one
#      reads about the other in the same place.
#   3. The file carries both worked examples: an http one that sets the cookie
#      knob to 1, and an https one that does not.
# ---------------------------------------------------------------------------
check_envpair() {
envpair_log=""
if envpair_log="$(python3 - <<'PYENVPAIR'
import io
import re

ENV = ".env.example"

SITE = "SITE_ADDRESS"
COOKIE = "CADUS_WEB_INSECURE_COOKIE"

problems = []
text = io.open(ENV, encoding="utf-8").read()
lines = text.splitlines()


def active(key):
    """The value of the last uncommented `KEY=value` line, or None."""
    found = None
    for line in lines:
        stripped = line.strip()
        if stripped.startswith(key + "="):
            found = stripped[len(key) + 1:].strip()
    return found


def block_of(key):
    """The block that documents one key.

    A block is the run of lines above the key, up to the first blank line: the
    comment paragraph and any key that sits in the same paragraph. The ACTIVE
    line of the key comes first. A key that only appears commented out, which is
    what an https deployment does with the cookie knob, falls back to the first
    commented line.
    """
    index = None
    for position, line in enumerate(lines):
        if line.strip().startswith(key + "="):
            index = position
    if index is None:
        for position, line in enumerate(lines):
            if line.strip().lstrip("#").strip().startswith(key + "="):
                index = position
                break
    if index is None:
        return ""
    start = index
    while start > 0 and lines[start - 1].strip() != "":
        start -= 1
    return "\n".join(lines[start:index + 1])


site = active(SITE)
cookie = active(COOKIE)

if site is None:
    problems.append(ENV + " sets no " + SITE)
else:
    http_only = site.startswith(":") or site.startswith("http://")
    if http_only and cookie != "1":
        problems.append(
            ENV + " serves http (" + SITE + "=" + site + ") and carries "
            + (COOKIE + "=" + cookie if cookie is not None else "no active " + COOKIE)
            + "; a browser discards the Secure __Host- cookie on http, so the login"
            " answers 200 and the next authed write answers 401. Set " + COOKIE + "=1"
        )
    if not http_only and cookie == "1":
        problems.append(
            ENV + " serves https (" + SITE + "=" + site + ") and carries " + COOKIE
            + "=1, which drops Secure from the session cookie of a public site"
        )

site_block = block_of(SITE)
cookie_block = block_of(COOKIE)
if COOKIE not in site_block:
    problems.append(
        "the " + SITE + " block of " + ENV + " does not name " + COOKIE
        + "; the two keys are one decision and must be documented as a pair"
    )
if SITE not in cookie_block:
    problems.append(
        "the " + COOKIE + " block of " + ENV + " does not name " + SITE
        + "; the two keys are one decision and must be documented as a pair"
    )

# The two worked examples. Each is a line pair inside a comment block: one
# SITE_ADDRESS line and one CADUS_WEB_INSECURE_COOKIE line for that scheme.
examples = re.findall(
    r"^#\s*(?:" + SITE + r")\s*=\s*(\S+)[^\n]*\n#\s*(?:" + COOKIE + r")\s*=\s*(\S+)",
    text,
    re.M,
)
http_example = [pair for pair in examples if pair[0].startswith((":", "http://"))]
https_example = [pair for pair in examples if not pair[0].startswith((":", "http://"))]

if not any(pair[1] == "1" for pair in http_example):
    problems.append(
        ENV + " carries no http example that pairs an http " + SITE + " with "
        + COOKIE + "=1"
    )
if not any(pair[1] == "0" for pair in https_example):
    problems.append(
        ENV + " carries no https example that pairs a domain in " + SITE + " with "
        + COOKIE + "=0"
    )

for line in problems:
    print(line)
PYENVPAIR
)"; then
    if [ -n "$envpair_log" ]; then
        printf 'FAIL: envpair  -- %s\n' "$envpair_log"
        rc=1
    else
        echo "PASS: envpair  -- .env.example pairs SITE_ADDRESS with CADUS_WEB_INSECURE_COOKIE, documents both keys together, and carries the http example (=1) and the https example (=0)"
    fi
else
    echo "FAIL: envpair  -- the .env.example pair check did not run"
    printf '%s\n' "$envpair_log"
    rc=1
fi
}

