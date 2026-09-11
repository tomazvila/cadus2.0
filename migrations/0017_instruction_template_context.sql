-- Bind instruction approvals to the complete currently eligible template bank.
ALTER TABLE content_store ADD COLUMN approved_template_context_digest text;

-- Ordinary practice uses every current approved template. Finite practice uses
-- its newest current approved template. Ordering matches serving selection.
-- Both functions use the caller's privileges and preserve tenant policies.
CREATE FUNCTION public.cadus_template_bank(review_kp text, review_policy text)
RETURNS text[] LANGUAGE sql STABLE SECURITY INVOKER AS $$
    SELECT array_agg(digest ORDER BY digest)
    FROM (
        SELECT digest, row_number() OVER (
            ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest
        ) AS position
        FROM public.content_store
        WHERE kp_id = review_kp AND kind = 'template' AND status = 'approved'
          AND approved_policy_digest IS NOT DISTINCT FROM review_policy
    ) AS eligible
    WHERE review_policy IS NULL OR position = 1
$$;

CREATE FUNCTION public.cadus_template_context(review_kp text, review_policy text)
RETURNS text LANGUAGE sql STABLE SECURITY INVOKER AS $$
    SELECT encode(sha256(convert_to(
        to_jsonb(public.cadus_template_bank(review_kp, review_policy))::text, 'UTF8'
    )), 'hex')
$$;
