-- psql -X -qAt -v ON_ERROR_STOP=1 -v user_id=<uuid> -f this-file.sql
-- Reads one complete tenant history, including correction evidence.
BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL app.user_id = :'user_id';
SELECT json_build_object(
    'record_type', 'snapshot', 'user_id', :'user_id'::uuid,
    'event_count', count(*), 'through_seq', coalesce(max(seq), 0))
FROM events WHERE user_id = :'user_id'::uuid;
SELECT json_build_object(
    'record_type', 'event', 'user_id', user_id, 'seq', seq,
    'schema_version', v, 'event_json', payload::text)
FROM events WHERE user_id = :'user_id'::uuid ORDER BY seq;
COMMIT;
