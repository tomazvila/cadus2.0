-- A server-gated semantic case survives approved template replacements.
-- Legacy rows remain NULL until their exact reviewed payload is matched.
ALTER TABLE serving_pool ADD COLUMN finite_case_id text;
