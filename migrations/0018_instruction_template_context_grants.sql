-- Context helpers are read-only and execute with each caller's privileges.
-- Migration 0006 revokes default PUBLIC function execution.
REVOKE ALL ON FUNCTION public.cadus_template_bank(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.cadus_template_context(text, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.cadus_template_bank(text, text) TO cadus_app, cadus_admin;
GRANT EXECUTE ON FUNCTION public.cadus_template_context(text, text) TO cadus_app, cadus_admin;
