-- 0027 granted plain INSERT on proof_owed to cadus_app, table-wide. Narrow it
-- to the three columns the request tier writes (crates/store/src/proof_grading.rs
-- `owe`): user_id, topic, kp. created_at keeps its DEFAULT and the app never
-- sets it, mirroring the column-scoped INSERT grants of 0025 and 0026.
REVOKE INSERT ON proof_owed FROM cadus_app;
GRANT INSERT (user_id, topic, kp) ON proof_owed TO cadus_app;
