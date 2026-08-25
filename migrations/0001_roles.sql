-- 0001_roles: extensions and the three cluster roles.
-- Requirements: C3 (multi-tenant Postgres, RLS), D9 (schema lineage from 1.0).
--
-- Roles are cluster-scoped, not database-scoped. A second database on the same
-- cluster runs this migration again, so each CREATE ROLE has a pg_roles guard.
-- The guard makes the statement idempotent. Two databases on one cluster run
-- this migration at the same time in tests, so the EXCEPTION clause also
-- absorbs the error of a lost race. Postgres reports it as duplicate_object or
-- as unique_violation on pg_authid, so the clause names both.
--
-- Extensions:
--   citext  -- case-folded email in users.email (1.0 lineage).
-- 1.0 also created pgcrypto for gen_random_uuid(). Postgres 13 and later ship
-- gen_random_uuid() in core, so 2.0 does not need pgcrypto.

CREATE EXTENSION IF NOT EXISTS citext;

-- cadus_owner is reserved for a deployment that runs the migrations as a
-- non-superuser owner. Findings #25 and #36: in the shipped compose stack the
-- migration runner is the postgres superuser, which owns every object and
-- bypasses row-level security, so cadus_owner owns nothing there. FORCE ROW
-- LEVEL SECURITY in 0006 protects the non-superuser owner shape only.
-- NOLOGIN: nothing authenticates as the owner today.
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cadus_owner') THEN
    CREATE ROLE cadus_owner NOLOGIN NOSUPERUSER;
  END IF;
EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL;
END $$;

-- cadus_app is the runtime role. RLS applies to it (NOSUPERUSER NOBYPASSRLS).
-- C3: the app refuses to start if its role bypasses RLS.
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cadus_app') THEN
    CREATE ROLE cadus_app LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;
  END IF;
EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL;
END $$;

-- cadus_admin runs cross-tenant sweeps (worker queue drains, full replay).
-- BYPASSRLS: the sweep reads every tenant.
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cadus_admin') THEN
    CREATE ROLE cadus_admin NOLOGIN NOSUPERUSER BYPASSRLS;
  END IF;
EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL;
END $$;

-- Make cadus_admin a member of cadus_app. In M0 the worker connects as
-- cadus_admin and holds BYPASSRLS for its whole life; it runs no SET ROLE.
-- The membership prepares the M5 shape: SET ROLE cadus_app inside each
-- per-tenant drain, so RLS is a backstop again for that unit of work.
GRANT cadus_app TO cadus_admin;
