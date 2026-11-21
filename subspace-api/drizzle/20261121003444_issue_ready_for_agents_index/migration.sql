-- Safe to run twice: supports the poll for ready, unassigned and unarchived issues.
CREATE INDEX IF NOT EXISTS "issues_ready_for_agents_idx" ON "issues" ("organisation_id") WHERE "ready_for_agents" and "assignee_id" is null and "archived_at" is null;
--> statement-breakpoint
-- An assigned issue is never ready for agents, even when an older API instance assigns it during
-- a rolling deployment. The trigger clears the flag rather than refusing the write.
CREATE OR REPLACE FUNCTION subspace_clear_assigned_ready_for_agents() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.assignee_id IS NOT NULL AND NEW.ready_for_agents THEN
    NEW.ready_for_agents := false;
  END IF;
  RETURN NEW;
END;
$$;
--> statement-breakpoint
CREATE OR REPLACE TRIGGER subspace_issue_assigned_not_ready
BEFORE INSERT OR UPDATE OF "assignee_id", "ready_for_agents" ON "issues"
FOR EACH ROW EXECUTE FUNCTION subspace_clear_assigned_ready_for_agents();
