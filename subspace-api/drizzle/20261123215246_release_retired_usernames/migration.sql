-- A retired username is released or handed on only by the username command, which sets
-- subspace.release_username for its own transaction. Deleting an organisation still removes its
-- retired names, through the foreign key's cascade.
CREATE OR REPLACE FUNCTION subspace_retired_username_guard() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF pg_trigger_depth() = 1
     AND coalesce(current_setting('subspace.release_username', true), '') <> 'on' THEN
    RAISE EXCEPTION 'A retired username is released only by the username command.'
      USING ERRCODE = 'insufficient_privilege';
  END IF;
  IF TG_OP = 'DELETE' THEN
    RETURN OLD;
  END IF;
  RETURN NEW;
END;
$$;
--> statement-breakpoint
CREATE OR REPLACE TRIGGER subspace_retired_usernames_guard
BEFORE UPDATE OR DELETE ON "retired_usernames"
FOR EACH ROW EXECUTE FUNCTION subspace_retired_username_guard();
