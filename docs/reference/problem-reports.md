# Verified problem reports
Learners can report a question or a submitted answer without leaving Cadus. A durable worker asks the self-hosted Qwen model to review the issue. Supported mathematical claims require independently checkable Lean evidence before Cadus publishes a correction.
## Coverage
Reports support lesson, review, drill, quiz, diagnostic, and integrated-task fields. An integrated report names one step or the final field and includes the scenario and given quantities. A report made before submission reviews content only and creates no learner attempt.
The server reconstructs the question and submitted answer from the learner's events or durable served state. Request bodies supply identifiers and an optional note; they cannot supply an authoritative question, answer key, grade, or user identity.
## Review and evidence
Each producer, independent formalizer, critic, and Qwen adjudicator receives a fresh context. The graph permits at most three candidate rounds and records bounded evidence. Mathematical verification supports bounded factor lists, exact numeric expressions, and polynomial identities.
The verifier parses a restricted data grammar and renders application-owned Lean templates. Model-generated programs, commands, tactics, and imports are never executed. Checked mathematics and independent interpretation review are separate evidence: Lean does not prove that a natural-language question was translated faithfully.
Automatic publication requires a proved candidate, a proved reported answer when one exists, independent interpretation and solution review, and positive and negative acceptance regressions. Unsupported mathematics, unresolved disagreement, unavailable services, or incomplete evidence produce an unresolved report.
## Corrections
A verified correction appends a versioned answer and solution. The original question and answer policy remain fixed. The correction identity includes the question, answer contract, curriculum digest, and grading-engine digest. It excludes the expected-answer value being repaired, so subsequent repairs remain in the same version history. The original expected answer remains in each report's evidence.
Publication compares the captured correction version with the latest version. Stale workers cannot overwrite a newer correction. Previously verified aliases are retained only under the same formal problem.
The web application uses the published answer before ordinary, diagnostic, or integrated grading. Checked aliases use trimmed exact matching; other submissions retain the existing deterministic equivalence rules.
## Learner results
Submitted-answer corrections append a native regrade with versioned, exact-sequence replacement metadata. Store replay applies these replacements before projection. Raw event export preserves the original events and the later correction evidence. External replay consumers must apply the store replacement overlay before projecting rich task-result corrections.
Completed lesson, review, and quiz results and their XP are recalculated from corrected attempts under the captured task policy. A lesson with insufficient evidence reopens rather than receiving an invented pass. Quiz answer buffers and integrated replay responses use the corrected results.
Integrated corrections update the selected field and derived grade totals and skills. Integrated tasks do not award XP under the existing grading model.
Diagnostic runs record their start, full balance state, policy, and placement boundary. A correction updates the matching run and its placement; it cannot spill into a restarted diagnostic. Historical diagnostic answers without a trustworthy run boundary remain unresolved.
The public result reports corrected_outcome only when a learner regrade was committed. The interface changes its displayed grade only after that confirmation.
## Assessment integrity
The server withholds report answers and solution details before submission and while a quiz or placement assessment remains open. This restriction also applies to direct report-polling requests. The frontend preserves a report receipt when the learner advances to the next question.
## Isolation and operation
Tenant row-level security protects reports and diagnostic runs. Web credentials cannot publish content or forge worker results. Verification traces are private to the worker's admin role.
The queue allows one active Qwen report globally, with a two-minute renewable lease, at most three recovery claims, and bounded per-learner submissions. Ordinary web requests make no model calls.
The verifier runs on an internal network with no host ports, database credentials, model credentials, host mounts, or Docker socket. Its read-only filesystem, restricted user, CPU, memory, process, and temporary-storage limits remain part of deployment.
Application-code, generator, hint-ladder, and Teach-page edits remain separate changes. See deploy/REPORT_WORKER.md for the runtime configuration.
