-- Bind approvals and generated pool rows to effective curriculum and engine.
ALTER TABLE content_store
    ADD COLUMN approved_curriculum_digest text,
    ADD COLUMN approved_review_engine_digest text;

ALTER TABLE serving_pool
    ADD COLUMN source_curriculum_digest text,
    ADD COLUMN source_review_engine_digest text;

CREATE INDEX content_store_current_review_idx
    ON content_store (kp_id, kind, approved_policy_digest,
                      approved_curriculum_digest, approved_review_engine_digest,
                      approved_at DESC, created_at DESC, digest)
    WHERE status = 'approved';

CREATE INDEX serving_pool_current_generation_idx
    ON serving_pool (user_id, kp_id, source_curriculum_digest,
                     source_review_engine_digest, created_at, id)
    WHERE claimed_at IS NULL;

DROP FUNCTION public.cadus_template_context(text, text);
DROP FUNCTION public.cadus_template_bank(text, text);

-- Only approvals current under all trusted dimensions belong to a live bank.
CREATE FUNCTION public.cadus_template_bank(
    review_kp text,
    review_policy text,
    review_curriculum text,
    review_engine text
) RETURNS text[] LANGUAGE sql STABLE SECURITY INVOKER AS $$
    SELECT array_agg(digest ORDER BY digest)
    FROM (
        SELECT digest, row_number() OVER (
            ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest
        ) AS position
        FROM public.content_store
        WHERE kp_id = review_kp AND kind = 'template' AND status = 'approved'
          AND approved_policy_digest IS NOT DISTINCT FROM review_policy
          AND approved_curriculum_digest = review_curriculum
          AND approved_review_engine_digest = review_engine
    ) AS eligible
    WHERE review_policy IS NULL OR position = 1
$$;

CREATE FUNCTION public.cadus_template_context(
    review_kp text,
    review_policy text,
    review_curriculum text,
    review_engine text
) RETURNS text LANGUAGE sql STABLE SECURITY INVOKER AS $$
    SELECT encode(sha256(convert_to(
        to_jsonb(public.cadus_template_bank(
            review_kp, review_policy, review_curriculum, review_engine
        ))::text, 'UTF8'
    )), 'hex')
$$;

-- Snapshot a template candidate's prospective serving set. Ordinary banks
-- union the candidate; a finite candidate atomically replaces the one slot.
CREATE FUNCTION public.cadus_template_context_after(
    review_kp text,
    review_policy text,
    candidate_digest text,
    review_curriculum text,
    review_engine text
) RETURNS text LANGUAGE sql STABLE SECURITY INVOKER AS $$
    SELECT encode(sha256(convert_to(to_jsonb(
        CASE WHEN review_policy IS NOT NULL THEN ARRAY[candidate_digest]
        ELSE (
            SELECT array_agg(digest ORDER BY digest)
            FROM (
                SELECT unnest(COALESCE(public.cadus_template_bank(
                    review_kp, review_policy, review_curriculum, review_engine
                ), ARRAY[]::text[])) AS digest
                UNION
                SELECT candidate_digest
            ) AS prospective
        ) END
    )::text, 'UTF8')), 'hex')
$$;

REVOKE ALL ON FUNCTION public.cadus_template_bank(text,text,text,text) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.cadus_template_context(text,text,text,text) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.cadus_template_context_after(text,text,text,text,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.cadus_template_bank(text,text,text,text) TO cadus_app, cadus_admin;
GRANT EXECUTE ON FUNCTION public.cadus_template_context(text,text,text,text) TO cadus_app, cadus_admin;
GRANT EXECUTE ON FUNCTION public.cadus_template_context_after(text,text,text,text,text) TO cadus_app, cadus_admin;
