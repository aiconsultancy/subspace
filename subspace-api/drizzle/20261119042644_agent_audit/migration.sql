CREATE TABLE "audit_logs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"occurred_at" timestamp with time zone DEFAULT now() NOT NULL,
	"action" text NOT NULL,
	"actorType" text NOT NULL,
	"actor_id" uuid,
	"actor_display_name" text,
	"actor_rank" text,
	"source" text NOT NULL,
	"entity_type" text NOT NULL,
	"entity_id" text,
	"project_id" uuid,
	"summary" text NOT NULL,
	"changes" jsonb NOT NULL
);
--> statement-breakpoint
ALTER TABLE "issue_activity" ADD COLUMN "source" text DEFAULT 'rest' NOT NULL;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "agent_rank" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "agent_fleet" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "agent_ship" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "agent_model" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "registered_by_id" uuid;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "deactivated_by_id" uuid;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "deactivated_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "version" integer DEFAULT 1 NOT NULL;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "created_at" timestamp with time zone DEFAULT now() NOT NULL;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "updated_at" timestamp with time zone DEFAULT now() NOT NULL;--> statement-breakpoint
CREATE INDEX "audit_logs_organisation_id_occurred_at_id_index" ON "audit_logs" ("organisation_id","occurred_at","id");--> statement-breakpoint
CREATE UNIQUE INDEX "user_agent_name" ON "users" ("organisation_id",lower("display_name")) WHERE "agent_rank" is not null;--> statement-breakpoint
ALTER TABLE "audit_logs" ADD CONSTRAINT "audit_logs_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "users" ADD CONSTRAINT "users_KVgVvBDRAZdY_fkey" FOREIGN KEY ("organisation_id","registered_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "users" ADD CONSTRAINT "users_1ZSZOSE3zudA_fkey" FOREIGN KEY ("organisation_id","deactivated_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "users" ADD CONSTRAINT "agent_members" CHECK ("agent_rank" is null or (
      "role" = 'member' and "agent_rank" in ('rearAdmiral', 'captain', 'crew') and
      "agent_fleet" is not null and "agent_model" is not null and "registered_by_id" is not null
    ));
--> statement-breakpoint
CREATE OR REPLACE FUNCTION subspace_notify_issue_activity() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  hint jsonb;
  source_project record;
BEGIN
  SELECT jsonb_build_object(
      'organisationId', NEW.organisation_id,
      'projectId', i.project_id,
      'projectKey', p.key,
      'issueId', i.id,
      'identifier', i.identifier,
      'parentIdentifier', parent.identifier,
      'version', i.version,
      'auditId', NEW.id,
      'action', CASE NEW.kind
        WHEN 'issue.created' THEN 'issueCreated'
        WHEN 'issue.changed' THEN 'issueUpdated'
        WHEN 'issue.state-derived' THEN 'issueUpdated'
        WHEN 'issue.archived' THEN 'issueArchived'
        WHEN 'issue.unarchived' THEN 'issueUnarchived'
        WHEN 'issue.moved' THEN 'issueMoved'
        WHEN 'comment.created' THEN 'commentCreated'
        WHEN 'decision.logged' THEN 'decisionLogged'
        WHEN 'item.created' THEN 'issueTodoCreated'
        WHEN 'item.removed' THEN 'issueTodoDeleted'
        WHEN 'item.moved' THEN 'issueTodosReordered'
        WHEN 'item.edited' THEN 'issueTodoUpdated'
        WHEN 'item.completed' THEN 'issueTodoCompleted'
        WHEN 'item.reopened' THEN 'issueTodoReopened'
        WHEN 'comment.edited' THEN 'commentEdited'
        WHEN 'comment.deleted' THEN 'commentDeleted'
        WHEN 'comment.resolved' THEN 'commentResolutionChanged'
        WHEN 'pull-request.linked' THEN 'pullRequestLinked'
        WHEN 'pull-request.updated' THEN 'pullRequestUpdated'
        WHEN 'pull-request.unlinked' THEN 'pullRequestUpdated'
        ELSE 'issueUpdated'
      END,
      'actorType', CASE WHEN NEW.actor_id IS NULL THEN 'integration'
        WHEN u.agent_rank IS NOT NULL THEN 'agent' ELSE 'human' END,
      'occurredAt', NEW.created_at
    )
    INTO hint
  FROM issues i
  JOIN projects p ON p.id = i.project_id AND p.organisation_id = i.organisation_id
  LEFT JOIN users u ON u.id = NEW.actor_id AND u.organisation_id = NEW.organisation_id
  LEFT JOIN issues parent ON parent.id = i.parent_id AND parent.organisation_id = i.organisation_id
  WHERE i.id = NEW.issue_id AND i.organisation_id = NEW.organisation_id;
  IF hint IS NOT NULL THEN
    PERFORM pg_notify('subspace_issues', hint::text);
  END IF;
  IF NEW.kind = 'issue.moved' AND NEW.data ? 'sourceProjectId' THEN
    SELECT p.id, p.key INTO source_project FROM projects p
    WHERE p.id = (NEW.data->>'sourceProjectId')::uuid AND p.organisation_id = NEW.organisation_id;
    IF FOUND THEN
      PERFORM pg_notify('subspace_issues', (hint || jsonb_build_object(
        'projectId', source_project.id, 'projectKey', source_project.key
      ))::text);
    END IF;
  END IF;
  RETURN NEW;
END;
$$;
--> statement-breakpoint
CREATE FUNCTION subspace_audit_issue_activity() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO audit_logs (id, organisation_id, occurred_at, action, actor_type, actor_id,
    actor_display_name, actor_rank, source, entity_type, entity_id, project_id, summary, changes)
  SELECT NEW.id, NEW.organisation_id, NEW.created_at,
    CASE NEW.kind
      WHEN 'issue.created' THEN 'issueCreated'
      WHEN 'issue.changed' THEN CASE WHEN NEW.data ? 'previousStateId' OR NEW.source = 'github'
        THEN 'issueStatusChanged' ELSE 'issueUpdated' END
      WHEN 'issue.state-derived' THEN 'issueStatusChanged'
      WHEN 'issue.archived' THEN 'issueArchived'
      WHEN 'issue.unarchived' THEN 'issueUnarchived'
      WHEN 'issue.moved' THEN 'issueMoved'
      WHEN 'comment.created' THEN 'issueCommentAdded'
      WHEN 'comment.edited' THEN 'issueCommentEdited'
      WHEN 'comment.deleted' THEN 'issueCommentDeleted'
      WHEN 'comment.resolved' THEN 'issueCommentResolutionChanged'
      WHEN 'decision.logged' THEN 'decisionLogged'
      WHEN 'item.created' THEN 'issueTodoAdded'
      WHEN 'item.removed' THEN 'issueTodoDeleted'
      WHEN 'item.edited' THEN 'issueTodoUpdated'
      WHEN 'item.completed' THEN 'issueTodoCompleted'
      WHEN 'item.reopened' THEN 'issueTodoReopened'
      WHEN 'item.moved' THEN 'issueTodosReordered'
      ELSE NEW.kind
    END,
    CASE WHEN NEW.actor_id IS NULL THEN 'integration'
      WHEN u.agent_rank IS NOT NULL THEN 'agent' ELSE 'human' END,
    NEW.actor_id, COALESCE(u.display_name, 'GitHub'), u.agent_rank, NEW.source,
    'Issue', NEW.issue_id::text, i.project_id, NEW.kind, NEW.data
  FROM issues i
  LEFT JOIN users u ON u.id = NEW.actor_id AND u.organisation_id = NEW.organisation_id
  WHERE i.id = NEW.issue_id AND i.organisation_id = NEW.organisation_id;
  RETURN NEW;
END;
$$;
--> statement-breakpoint
CREATE TRIGGER subspace_issue_activity_audit
AFTER INSERT ON issue_activity FOR EACH ROW EXECUTE FUNCTION subspace_audit_issue_activity();
