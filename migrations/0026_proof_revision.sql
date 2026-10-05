-- The proof revision loop (docs/DECISIONS.md, D-PR1): a written proof in a
-- lesson closes its knowledge point on a model PASS, a needs-revision verdict
-- sends the learner back to the same problem, and every resubmission is a new
-- graded job linked to the one it revises.
--
-- The chain is a linked list of proof_grading_jobs rows: `revision_of` names
-- the job a draft revises, so the root has none and the head has no
-- successor. Each row keeps its own verdict and feedback, so the whole
-- history of one problem reads back from the table.
--
-- context   where the draft was written: 'lesson' (the blocking loop),
--           'review' (non-blocking; revised from the proofs list), 'quiz'
--           (graded after the reveal, no loop), 'selfcheck' (an explanation
--           self-check, no loop), 'legacy' (every row written before this
--           migration).
-- revision  how many needs-revision verdicts the chain used before this draft
--           (0 for the first draft). The cap is 2 revisions.
-- rewrite   the unaided rewrite after the cap; it closes the chain.
-- seen_at   when the learner first saw the verdict of this row.
-- revealed_at  when the reference solution was shown after the cap.
-- closed_at the chain is over (set on the head): the pass was applied, or the
--           rewrite was submitted.
-- disputed_at, dispute_note  the learner says the grade is wrong; the row
--           then lists in GET /api/admin/ungraded.
-- override_verdict  the human verdict of a disputed row ('pass' or
--           'needs_revision'); it replaces the model verdict.
ALTER TABLE proof_grading_jobs
    ADD COLUMN revision_of uuid NULL REFERENCES proof_grading_jobs(id) ON DELETE SET NULL,
    ADD COLUMN revision integer NOT NULL DEFAULT 0 CHECK (revision BETWEEN 0 AND 10),
    ADD COLUMN context text NOT NULL DEFAULT 'legacy'
        CHECK (context IN ('legacy','lesson','review','quiz','selfcheck')),
    ADD COLUMN rewrite boolean NOT NULL DEFAULT false,
    ADD COLUMN seen_at timestamptz NULL,
    ADD COLUMN revealed_at timestamptz NULL,
    ADD COLUMN closed_at timestamptz NULL,
    ADD COLUMN disputed_at timestamptz NULL,
    ADD COLUMN dispute_note text NULL CHECK (dispute_note IS NULL OR length(dispute_note) <= 2000),
    ADD COLUMN override_verdict text NULL CHECK (override_verdict IN ('pass','needs_revision'));

-- One successor per job: a chain never forks.
CREATE UNIQUE INDEX proof_grading_jobs_one_successor
    ON proof_grading_jobs (revision_of) WHERE revision_of IS NOT NULL;
CREATE INDEX proof_grading_jobs_open
    ON proof_grading_jobs (user_id, created_at) WHERE closed_at IS NULL;

-- The request tier writes the chain columns of a new row and the learner-side
-- marks of an existing one. Status, attempts, result and the timestamps of the
-- worker stay out of its reach.
GRANT INSERT (revision_of, revision, context, rewrite) ON proof_grading_jobs TO cadus_app;
GRANT UPDATE (seen_at, revealed_at, closed_at, disputed_at, dispute_note, override_verdict)
    ON proof_grading_jobs TO cadus_app;
