-- Several written items per knowledge point (D-PR1): a point owes one written
-- item per `none`-proof or `written` exemplar, so the owed fact is keyed by
-- the item as well. problem_hash is `problem_text_hash` of the item's
-- statement. A row of 0027 keeps the empty hash and stays valid as the owed
-- item of a point with one written item.
--
-- The table keeps its row-level security and its grants (0027, 0028); only the
-- column INSERT grant of cadus_app takes the new column.
ALTER TABLE proof_owed ADD COLUMN problem_hash text NOT NULL DEFAULT '';
ALTER TABLE proof_owed DROP CONSTRAINT proof_owed_pkey;
ALTER TABLE proof_owed ADD PRIMARY KEY (user_id, topic, kp, problem_hash);
REVOKE INSERT ON proof_owed FROM cadus_app;
GRANT INSERT (user_id, topic, kp, problem_hash) ON proof_owed TO cadus_app;
