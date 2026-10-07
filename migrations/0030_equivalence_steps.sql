-- The background check owns the verdict (owner report 10, 2026-10-07).
--
--   * steps      -- the learner-facing lines the worker appends while it works,
--                   [{"text": "..."}], in order. The poll route returns them.
--   * landed_at  -- when the web tier rewrote the stored attempt after an
--                   accepted verdict. The rewrite runs once per job: the
--                   UPDATE that sets this column wins the right to run it.
--   * landed_xp  -- the XP that rewrite awarded, so a reload reads the same
--                   number the first poll showed.
--
-- The worker (cadus_admin) already holds every privilege on the table. The
-- request tier gets UPDATE on the two landing columns only.
ALTER TABLE equivalence_jobs
    ADD COLUMN steps      jsonb NOT NULL DEFAULT '[]'
        CHECK (jsonb_typeof(steps) = 'array' AND octet_length(steps::text) <= 16384),
    ADD COLUMN landed_at  timestamptz NULL,
    ADD COLUMN landed_xp  double precision NULL;
GRANT UPDATE (landed_at, landed_xp) ON equivalence_jobs TO cadus_app;
