-- Bind AI or optional human review to the current trusted exercise policy.
-- NULL denotes an ordinary knowledge point without a finite-domain policy.
-- Existing finite documents require review against their current policy.
ALTER TABLE content_store ADD COLUMN approved_policy_digest text;
CREATE INDEX content_store_approved_policy
    ON content_store (kp_id, kind, approved_policy_digest, approved_at)
    WHERE status = 'approved';
