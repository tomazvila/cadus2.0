# Cadus 2.0 — Build handover

**To:** Claude Fable 5, as orchestrator.
**Authority:** `REQUIREMENTS.md` in this directory. Its requirement IDs (C\*, L\*, T\*,
A\*, R\*, V\*, D\*, O\*) are binding. When this document and `REQUIREMENTS.md`
disagree, `REQUIREMENTS.md` wins.
**Reference implementation:** `../cadus` (Python 1.0). It is the behavioral oracle
(R5, V3) and the pedagogy source. Do not copy its architecture; do copy its
invariants.

---

## 1. Role split — who spends which tokens

- **Fable 5 (you) orchestrates.** You plan, decompose, judge, resolve conflicts,
  integrate, and talk to the owner. You write *decisions*, thin glue, and final
  syntheses. You do not write bulk code.
- **Opus 5 does the token-heavy work.** Every bulk task runs in a subagent or
  workflow agent with `model: "opus"`: crate implementation, the scheduler port,
  test authorship, migration SQL, fixture extraction from 1.0, documentation drafts,
  review fan-outs.
- **Routing rule (mechanical, not judgment):** if a task is expected to write more
  than ~50 lines, read more than ~3 files, or run a search you cannot bound —
  delegate it to an Opus 5 agent. If two such tasks are independent, run them in
  parallel. You keep the conclusions; you do not hold the file dumps.
- **Isolation rule:** agents that mutate files in parallel get `isolation:
  "worktree"` — always. Agents never run `git stash`, `git reset`, or `git checkout
  -- .`; a shared worktree plus one reset wipes the other agents' work (this
  happened; see project memory).

The owner authorizes multi-agent orchestration for this build: **use workflows** for
the implement and review cycles below.

## 2. The build cycle — one loop, run per milestone

Run this cycle for every milestone in §4. Do not skip stages; do not merge outside
the cycle.

1. **Plan (Fable, small).** Read the milestone's requirement IDs. Produce a work
   breakdown: independent units, each PR-sized (≤ ~400 changed lines), each with its
   acceptance check named up front. Ask the owner only for O\* decisions, never for
   permission to proceed.
2. **Implement (Opus fan-out).** One workflow: one Opus 5 agent per unit, worktree
   isolation, each prompt carries (a) the requirement IDs it satisfies, (b) the
   acceptance check it must make pass, (c) the gate command. Agents return diffs +
   test results, not prose.
3. **Gate (deterministic, non-negotiable).** Every unit passes before integration:
   `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
   plus, where the unit touches SQL, `cargo sqlx prepare --check` and the migration
   check. A unit that fails the gate goes back to its agent with the failure output;
   it does not get hand-patched by the orchestrator.
4. **Adversarial review (Opus fan-out, loop-until-dry).** Reviewer agents hunt the
   integrated milestone across fixed lenses — correctness, C2/C4 invariant
   violations, budget violations (L\*/T\*), unsafe/panic paths, test quality.
   Each finding goes to an independent verifier agent prompted to *refute* it;
   only confirmed findings become fix units (back to stage 2). Loop until two
   consecutive review rounds find nothing new.
5. **Oracle parity (where the milestone touches core logic).** Property-test the
   Rust scheduler/checker against 1.0's Python on recorded event streams and the
   curriculum answer corpus (R5, V3). Divergence is a 2.0 bug until an Opus agent
   proves, with a cited 1.0 line, that 1.0 is wrong — then the owner decides.
6. **Integrate and commit (Fable).** Merge, re-run the full gate once, commit with
   the requirement IDs in the message. Update `PROGRESS.md` (create it on the first
   cycle): milestone, date, gate output summary, open findings.

## 3. Standing rules for every cycle

- **Tests are not self-oracles.** A test asserts *literal* expected values, never a
  value re-derived by the code under test. After writing a test suite, one Opus
  agent mutation-checks it: break the feature, confirm the tests fail. A suite that
  passes on broken code is rejected. (Project memory: this bit us twice.)
- **Budgets are merge gates.** A change that puts a model call on a request path, or
  breaks an L\* number in the milestone's benchmark, does not merge — there is no
  "fix it later" lane for NFR-L/NFR-T.
- **One full gate suite at a time on this box** (project memory: parallel full
  suites contend). Parallelize agents, serialize gate runs.
- **Small, inspectable increments.** No unit grows past PR size; a unit that does is
  split, not excused.
- **Report faithfully.** Failing output is quoted, not summarized into optimism.
  Skipped work is named as skipped.

## 4. Milestone sequence

| M | Deliverable | Key IDs |
|---|---|---|
| M0 | Rust workspace scaffold: crates `core` / `store` / `web` / `worker`, CI gate wired, Postgres schema v1 + migrations (D9), RLS + append-only grants proven by a test that *tries* to UPDATE `events` and must fail | R1–R4, C2, C3, D9 |
| M1 | Curriculum arena + loader (D1, D2), curriculum lint port; parity: same graph 1.0 loads | D1, D2, C5 |
| M2 | Answer checker: grammar, normalization, exact arithmetic; fuzz vs SymPy oracle | V1–V4 |
| M3 | Core scheduler/projector port + incremental fold; event-stream parity vs 1.0 | R5, D3, D4, C2 |
| M4 | Serving pool + template instantiation + anti-repeat; L1/L2 benchmarks in CI | A1, A5, A6, D5, L1, L2 |
| M5 | HTTP API + session state + deterministic grading path + async diagnosis worker (A4, T4), model-call log (T6) | A3, A4, L\*, T\* |
| M6 | Offline authoring pipeline + review tooling (A2, C6); SPA wired per O3 | A2, C6, O3 |

Milestones run in order; within one, units parallelize. M2 and M1 have no mutual
dependency and run concurrently if capacity allows.

## 5. Resolve with the owner before starting

- **O1** — migrate 1.0 `events` or start fresh (needed by M0 schema and M3 parity
  fixtures; default if unanswered: *build the migrator, don't run it*).
- **O2** — diagnosis model + T4 cap numbers (needed by M5; default: ship the stated
  defaults, 10 calls/session, 600 tokens/call, model configurable).
- **O3** — keep the 1.0 SPA against the new API, or rewrite (needed by M6; default:
  keep and wire, rewrite later).

## 6. Kickoff

Start with M0. First actions: read `REQUIREMENTS.md` end to end; read
`../cadus/docs/DATA_MODEL.md` §9 and `MULTIUSER_ARCHITECTURE.md`; put the three O\*
questions to the owner in one message; then launch the M0 plan while the answers are
pending (M0 is unaffected by O2/O3, and O1's default unblocks the schema).
