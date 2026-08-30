-- 0012_content_prompt_digest: the prompt that authored one stored document.
-- Requirements: C6 (a human review gate binds approval to the digest), A2 (the
-- offline authoring pass), T3 (the money of one authored document).
--
-- Spec `docs/reference/authoring-and-spa-1.0-spec.md` section 2.2, "Prompt
-- digest": 1.0 folds the prompt digest into the CACHE KEY, so a prompt edit
-- retires every stored template (`problem_templates.py:1129-1163`). 2.0 stores
-- the digest as a column on the row, because the C6 approval binds to the
-- CONTENT. A prompt edit therefore marks the affected rows for re-authoring and
-- never silently unapproves one.
--
-- The column is nullable, and NULL means "the prompt that wrote this row is not
-- recorded". A NULL is NOT stale: every row written before this migration
-- carries one, and a pass that read a NULL as stale would re-author the whole
-- bank on the first run after the deployment. No backfill is needed.
--
-- 0006_grants_rls leaves cadus_app with SELECT on content_store and gives
-- cadus_admin full DML. A new column inherits the table-level grant, so this
-- migration adds no GRANT and the runtime role still cannot write the digest.
-- content_store is outside row-level security: it holds curriculum content and
-- not learner data (docs/SCHEMA.md).

ALTER TABLE content_store ADD COLUMN prompt_digest text;

-- The stale read asks for one kind and one prompt digest over approved rows.
CREATE INDEX content_store_kind_prompt_digest
    ON content_store (kind, prompt_digest);
