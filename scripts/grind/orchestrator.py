#!/usr/bin/env python3
"""CADUS 2.0 full-curriculum orchestrator (H-8, ISSUES.md).

Evaluation infrastructure for the grind: it drives a learner through whole
courses over the live API, authoring and approving content when the plan is
blocked. It holds NO secret — the session cookie lives at COOKIE, and the
learner id is the eval account, not a credential.

Configuration (environment, with the defaults this box uses):
  CADUS_GRIND_BASE    the site origin
  CADUS_GRIND_COOKIE  the curl cookie file of the eval learner
  CADUS_GRIND_USER    the eval learner's user id
  CADUS_GRIND_LOG     the append-only log path

Loop:
  1. grind sessions (grind.py logic) while tasks are available
  2. when the plan is blocked: author content for blocked topics
     (deepseek/deepseek-chat via DeepInfra), approve it, re-stamp contexts
  3. make reviews due when the plan is empty
  4. enroll the next course at 100%

H-8(a): a serve error of code `unknown_task means the plan this round read is
stale against the log (the service replanned under us). The round re-fetches
the plan and walks it again instead of skipping the task — the earlier
behavior burned 962 serve errors in one grind log and silently dropped tasks.
"""
import json, subprocess, time, sys, re, glob, os, datetime, random

BASE = os.environ.get("CADUS_GRIND_BASE", "https://cadus.homelab.tomazvi.la")
COOKIE = os.environ.get("CADUS_GRIND_COOKIE", "/tmp/cadus_cookies.txt")
ORIGIN = f"Origin: {BASE}"
USER = os.environ.get("CADUS_GRIND_USER", "6440c036-a358-4bd8-9c05-5df93ae65123")
COURSES = ["probability-statistics", "precalculus", "discrete-mathematics"]
MODEL_ENV = ["-e", "OPENAI_MODEL=deepseek/deepseek-chat",
             "-e", "OPENROUTER_PROVIDER_ORDER=deepinfra"]
LOG = open(os.environ.get("CADUS_GRIND_LOG", "/tmp/orchestrator.log"), "a", buffering=1)

def log(*a):
    msg = " ".join(str(x) for x in a)
    LOG.write(f"[{time.strftime('%H:%M:%S')}] {msg}\n")
    print(msg, flush=True)

def api(method, path, data=None):
    cmd = ["curl", "-sS", "--max-time", "60", "-b", COOKIE, "-H", ORIGIN, "-X", method, f"{BASE}{path}"]
    if data is not None:
        cmd += ["-H", "Content-Type: application/json", "-d", json.dumps(data)]
    r = subprocess.run(cmd, capture_output=True, text=True)
    try:
        return json.loads(r.stdout)
    except Exception:
        return {"_raw": (r.stdout or "")[:200]}

def db(sql):
    r = subprocess.run(["docker", "exec", "cadus2-db", "psql", "-U", "cadus_admin",
                        "-d", "cadus", "-t", "-A", "-c", sql], capture_output=True, text=True)
    if r.returncode != 0:
        log("DBERR:", r.stderr[:200])
    return r.stdout.strip()

def db_stdin(sql):
    """Run SQL fed over stdin: the model rewrite outgrew the argv limit
    (Errno 7 Argument list too long) once the topics map passed ~128 KiB."""
    r = subprocess.run(["docker", "exec", "-i", "cadus2-db", "psql", "-U", "cadus_admin",
                        "-d", "cadus", "-v", "ON_ERROR_STOP=1", "-q", "-f", "-"],
                       input=sql, capture_output=True, text=True)
    if r.returncode != 0:
        log("DBERR:", r.stderr[:300])
    return r.stdout.strip()

def expected_for(task):
    return db(f"SELECT doc->'served'->'{task}'->'expected'->>'answer' "
              f"FROM web_states WHERE user_id='{USER}';") or None


def diag_expected(topic):
    """The diagnostic exemplar answer for a topic, read from the curriculum YAML."""
    for f in glob.glob("/home/deploy/dev/cadus2.0/curriculum/*/*.yaml"):
        txt = open(f).read()
        m = re.search(rf"- id: {re.escape(topic)}\n", txt)
        if not m:
            continue
        seg = txt[m.start():]
        nxt = re.search(r"\n  - id: ", seg[10:])
        seg = seg[: nxt.start() + 10] if nxt else seg[:12000]
        dm = re.search(r"diagnostic_exemplar:\s*\n\s+problem:[^\n]*\n\s+answer:\s*[\'\"]?(.+?)[\'\"]?\s*(?:\n|$)", seg)
        if dm:
            return dm.group(1).strip()
    return None


def run_placement(course):
    """Run the placement diagnostic for a never-placed learner (W-C3: the empty
    plan of a fresh learner offers the diagnostic; the walkthrough runs it).
    Answers come from each probe topic's diagnostic exemplar in the YAML."""
    started = api("POST", "/api/diag/start", {})
    if "error" in started or "probe" not in started:
        log("diag: refused", json.dumps(started)[:120])
        return 0
    n = 0
    while n < 45:
        probe = started.get("probe") or {}
        topic, pid = probe.get("topic"), probe.get("problem_id")
        if not topic or not pid:
            break
        ans = diag_expected(topic)
        if ans is None:
            log("diag: no exemplar key for", topic, "- stopping placement")
            break
        started = api("POST", "/api/diag/answer",
                      {"problem_id": pid, "answer": ans})
        n += 1
        if "error" in started:
            log("diag: answer refused", json.dumps(started)[:150])
            break
        if not started.get("probe"):
            break
    fin = api("POST", "/api/diag/finish", {})
    log(f"diag: answered {n}, finish={json.dumps(fin)[:80]}")
    api("POST", "/api/session/end", {})
    db(f"DELETE FROM session_plans WHERE user_id='{USER}';")
    return n

# ---------------- session work ----------------

def plan_action(plan: dict) -> str:
    """Classify a session plan into the driver's next move (W-C3: an empty plan
    is a DESIGNED status, not a dead end). One of:
      "walk"   — tasks to serve, or a quiz due that the quiz path serves;
      "author" — no tasks but the frontier is blocked: author the blocked
                 topics via the authoring branch, then re-fetch;
      "end"    — no tasks and the course is complete: normal completion;
      "idle"   — nothing due and the frontier open: nothing to do, end the
                 session honestly instead of looping.
    Missing keys are treated as empty/false."""
    if plan.get("tasks"):
        return "walk"
    if plan.get("quiz_due"):
        return "walk"
    if plan.get("blocked"):
        return "author"
    if plan.get("course_complete"):
        return "end"
    return "idle"


def answer_task(task, task_type, confirm, stats):
    wrong_next = (not confirm) and task_type in ("lesson", "drill") and random.random() < 0.03
    first = True
    last_pid = None
    repeats = 0
    for _iteration in range(60):
        if first:
            r = api("POST", f"/api/task/{task}/serve", {})
            first = False
            if "error" in r:
                log("SERVE_ERR", task, json.dumps(r["error"])[:150])
                return "serve_error"
        cur = r.get("next") or r
        pid, text = cur.get("problem_id"), cur.get("text")
        if not pid:
            return r.get("task_status", "no_problem")
        if pid == last_pid:
            repeats += 1
            if repeats >= 3:
                log("DEADLOCK", task, str(pid), (text or "")[:80])
                return "serve_deadlock"
        else:
            repeats = 0
            last_pid = pid
        exp = expected_for(task)
        tries = 0
        while exp is None and tries < 5:
            time.sleep(0.4)
            exp = expected_for(task)
            tries += 1
        if exp is None:
            log("NO_EXPECTED", task, str(pid), (text or "")[:80])
            exp = "0"
        inject = wrong_next and random.random() < 0.6
        ans = ("0" if exp != "0" else "1") if inject else exp
        time.sleep(0.15 if task_type != "drill" else 0.4)
        resp = api("POST", f"/api/task/{task}/answer", {"problem_id": pid, "answer": ans})
        stats["answers"] += 1
        if inject:
            stats["injected_wrong"] += 1
            wrong_next = False
        elif task_type != "quiz" and resp.get("correct") is not True \
                and resp.get("outcome") not in ("ungraded", None) \
                and resp.get("task_status") != "already_recorded":
            log("CONTENT_MISMATCH", json.dumps({
                "task": task, "text": text, "expected": exp, "submitted": ans,
                "outcome": resp.get("outcome"), "solution": resp.get("solution"),
            }, ensure_ascii=False)[:500])
            stats["mismatches"] += 1
        if "error" in resp:
            log("ANSWER_ERR", task, str(pid), json.dumps(resp["error"])[:150])
            return "answer_error"
        if task_type == "quiz":
            if resp.get("task_status") == "already_recorded":
                first = True
                continue
            if resp.get("quiz_complete"):
                api("POST", f"/api/task/{task}/quiz-result", {})
                stats["quizzes"] += 1
                return "quiz_done"
            first = True
            continue
        if resp.get("task_status") == "already_recorded":
            first = True
            continue
        nxt = resp.get("next")
        if nxt:
            r = {"next": nxt}
            continue
        st = resp.get("task_status")
        if st in ("task_passed", "task_failed"):
            stats["tasks_done"] += 1
            return st
        first = True
    log("ITERATION_CAP", task)
    return "capped"

def session_round(course):
    stats = {"answers": 0, "tasks_done": 0, "quizzes": 0, "injected_wrong": 0, "mismatches": 0,
             "plan_refetches": 0}
    r = api("POST", "/api/session/start", {})
    if "error" in r:
        return stats, f"start_error:{r['error'].get('code')}", []
    # H-8(a): a serve error usually means this round's plan is stale (the service
    # replanned under us, so the task id answers unknown_task). Re-fetch the plan
    # and walk it again instead of skipping the task. Three walks bound the loop.
    blocked = []
    for _walk in range(3):
        plan = api("GET", "/api/session/plan")
        tasks = [t for t in plan.get("tasks", []) if not t["progress"]["done"]]
        blocked = plan.get("blocked", [])
        if not tasks:
            api("POST", "/api/session/end", {})
            return stats, plan_action(plan), blocked
        stale = False
        for t in tasks:
            st = answer_task(t["task_id"], t["task_type"], bool(t.get("confirm")) or t["task_type"] == "review", stats)
            if st in ("serve_error", "answer_error"):
                stale = True
                break
        if not stale:
            api("POST", "/api/session/end", {})
            return stats, "ok", blocked
        stats["plan_refetches"] += 1
        log("STALE_PLAN", course, "refetch", stats["plan_refetches"])
    stats["tasks_skipped"] = stats.get("tasks_skipped", 0) + 1
    api("POST", "/api/session/end", {})
    return stats, "serve_error", blocked

# ---------------- content authoring ----------------

def kps_of_topic(topic):
    out = []
    for f in glob.glob("/home/deploy/dev/cadus2.0/curriculum/*/*.yaml"):
        txt = open(f).read()
        m = re.search(rf"- id: {re.escape(topic)}\n", txt)
        if m:
            seg = txt[m.start():]
            nxt = re.search(r"\n  - id: ", seg[10:])
            seg = seg[: nxt.start() + 10] if nxt else seg[:8000]
            kps = sorted(set(re.findall(r"- id: (kp\d+)", seg)), key=lambda s: int(s[2:]))
            if kps:
                return [f"{topic}/{k}" for k in kps]
    return [f"{topic}/kp1"]

AUTH_PASS = 0

def author_kp(kp):
    """One authoring pass for one knowledge point.

    F-grind-throughput: the 429 budget is PER MODEL POOL, not per account, so
    consecutive passes rotate across pools instead of queueing on one (the
    shared pools saturate for hours under evening load)."""
    global AUTH_PASS
    pools = [("deepseek/deepseek-chat", "deepinfra"),
             ("z-ai/glm-5.3-flash", "fireworks"),
             ("deepseek/deepseek-chat", "deepinfra,fireworks")]
    model, pool = pools[AUTH_PASS % len(pools)]
    AUTH_PASS += 1
    cmd = ["docker", "exec",
           "-e", f"OPENAI_MODEL={model}",
           "-e", f"OPENROUTER_PROVIDER_ORDER={pool}",
           "cadus2-worker", "sh", "-c",
        f"cadus-worker author --kp {kp} --kind template --kind teach "
        f"--budget-usd 30.00 --request-reserve-usd 0.25"]
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.stdout.count("gate accepted")

def approve_all_pending():
    lst = api("GET", "/api/admin/content")
    items = lst if isinstance(lst, list) else lst.get("items", lst.get("content", []))
    ok = 0
    for it in items:
        if it.get("status") != "pending":
            continue
        d = it["digest"]
        ctx = api("GET", f"/api/admin/content/{d}")
        body = {"policy_digest": ctx.get("policy_digest"),
                "template_context_digest": ctx.get("template_context_digest"),
                "curriculum_digest": ctx.get("curriculum_digest"),
                "review_engine_digest": ctx.get("review_engine_digest")}
        r = api("POST", f"/api/admin/content/{d}/approve", body)
        if "error" not in r:
            ok += 1
    return ok

def restamp_contexts():
    db("""UPDATE content_store
          SET approved_template_context_digest =
              public.cadus_template_context(kp_id, approved_policy_digest,
                                            approved_curriculum_digest, approved_review_engine_digest)
          WHERE status='approved' AND kind IN ('teach','hint_ladder');""")

def make_due():
    if os.environ.get("CADUS_GRIND_NO_MAKE_DUE"):
        log("make_due disabled (walkthrough honesty mode)")
        return
    db(f"UPDATE events SET ts = ts + (now() - (SELECT max(ts) FROM events WHERE user_id='{USER}')) WHERE user_id='{USER}';")
    m = db(f"SELECT model::text FROM learner_models WHERE user_id='{USER}';")
    if not m:
        return
    model = json.loads(m)
    now = datetime.datetime.now(datetime.UTC).replace(tzinfo=None)
    n = 0
    for t in model.get("topics", {}).values():
        try:
            iv = float(t.get("interval_days") or 1)
        except Exception:
            iv = 1
        t["t0"] = (now - datetime.timedelta(days=iv + 1)).strftime("%Y-%m-%dT%H:%M:%S.%fZ")
        n += 1
    safe = json.dumps(model).replace("'", "''")
    db_stdin(f"UPDATE learner_models SET model = '{safe}'::jsonb WHERE user_id='{USER}';\n"
             f"DELETE FROM session_plans WHERE user_id='{USER}';")
    log(f"make_due: {n} topics")

# ---------------- main ----------------

def main():
    # Walkthrough mode: any course id may be named on the command line; a course
    # outside COURSES becomes a single-course run (wave-3 per-learner walkthroughs).
    if len(sys.argv) > 1 and sys.argv[1] not in COURSES:
        COURSES[:] = [sys.argv[1]]
    idx = COURSES.index(sys.argv[1]) if len(sys.argv) > 1 and sys.argv[1] in COURSES else 0
    deadline = time.time() + float(sys.argv[2] if len(sys.argv) > 2 else 3600)
    authored_topics = set()
    accepted_by_topic = {}
    idle_rounds = 0
    placed = False
    while time.time() < deadline:
        course, s = current_course(idx)
        if course == "ALL_DONE":
            log("ALL COURSES COMPLETE")
            return
        stats, how, blocked = session_round(course)
        log(f"round [{course}]: {how} {stats}")
        if how == "idle":
            if not placed and run_placement(course):
                placed = True
                continue
            log("plan idle (nothing due, frontier open) — ending session")
            return
        if how == "end":
            # course complete per plan: enroll the next one, or finish
            nxt = COURSES[idx + 1] if idx + 1 < len(COURSES) else None
            if nxt is None:
                log("ALL COURSES COMPLETE")
                return
            r = api("POST", "/api/enroll", {"course": nxt})
            if "error" not in r:
                idx += 1
                authored_topics.clear()
                log("ENROLLED", nxt)
                continue
            log("enroll refused:", json.dumps(r["error"])[:150])
        if how == "ok" and stats["answers"] > 0:
            idle_rounds = 0
            continue
        # plan empty or blocked: author content for blocked topics first
        if blocked:
            topics = [b["topic"] for b in blocked]
            new = [t for t in topics if t not in authored_topics]
            if new:
                log("AUTHORING for", new)
                for t in new:
                    authored_topics.add(t)
                    for kp in kps_of_topic(t):
                        a = author_kp(kp)
                        log("  authored", kp, "docs:", a)
                ok = approve_all_pending()
                restamp_contexts()
                log(f"approved {ok} docs, contexts re-stamped")
                db(f"DELETE FROM session_plans WHERE user_id='{USER}';")
                continue
        # nothing blocked but no tasks: make reviews due
        make_due()
        idle_rounds += 1
        if idle_rounds >= 3:
            # try to advance the course
            progress = (api("GET", "/api/status").get("velocity") or {}).get("course_progress", 0)
            if progress >= 0.995:
                nxt = COURSES[idx + 1] if idx + 1 < len(COURSES) else None
                if nxt is None:
                    log("ALL COURSES COMPLETE")
                    return
                r = api("POST", "/api/enroll", {"course": nxt})
                if "error" not in r:
                    idx += 1
                    authored_topics.clear()
                    log("ENROLLED", nxt)
                else:
                    log("enroll refused:", json.dumps(r["error"])[:150])
                    make_due()
            else:
                log(f"progress {progress}% with empty plan — inspect manually")
                time.sleep(20)
        time.sleep(1)

def current_course(idx):
    s = api("GET", "/api/status")
    cid = (s.get("course") or {}).get("id")
    if cid in COURSES:
        return cid, s
    return COURSES[idx], s

if __name__ == "__main__":
    main()
