CREATE TABLE "github_deployment_statuses" (
	"organisation_id" uuid,
	"repository" text,
	"deployment_id" bigint,
	"status_id" bigint,
	"sha" text NOT NULL,
	"environment" text NOT NULL,
	"state" text NOT NULL,
	"occurred_at" timestamp with time zone NOT NULL,
	CONSTRAINT "github_deployment_statuses_pkey" PRIMARY KEY("organisation_id","repository","deployment_id","status_id")
);
--> statement-breakpoint
CREATE TABLE "github_poll_checkpoints" (
	"organisation_id" uuid,
	"repository" text,
	"observed_through" timestamp with time zone NOT NULL,
	CONSTRAINT "github_poll_checkpoints_pkey" PRIMARY KEY("organisation_id","repository")
);
--> statement-breakpoint
ALTER TABLE "project_repositories" ADD COLUMN "created_at" timestamp with time zone DEFAULT now() NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_pull_request_links" ADD COLUMN "completion" text DEFAULT 'none' NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_activity" ALTER COLUMN "actor_id" DROP NOT NULL;--> statement-breakpoint
CREATE INDEX "github_deployment_statuses_organisation_id_repository_occurred_at_index" ON "github_deployment_statuses" ("organisation_id","repository","occurred_at");--> statement-breakpoint
ALTER TABLE "github_deployment_statuses" ADD CONSTRAINT "github_deployment_statuses_C0EDX9XXqd29_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "github_poll_checkpoints" ADD CONSTRAINT "github_poll_checkpoints_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;
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
        WHEN 'item.updated' THEN 'issueTodoUpdated'
        WHEN 'pull-request.linked' THEN 'pullRequestLinked'
        WHEN 'pull-request.updated' THEN 'pullRequestUpdated'
        WHEN 'pull-request.unlinked' THEN 'pullRequestUpdated'
        ELSE 'issueUpdated'
      END,
      'actorType', CASE WHEN NEW.actor_id IS NULL THEN 'system' ELSE 'human' END,
      'occurredAt', NEW.created_at
    )
    INTO hint
  FROM issues i
  JOIN projects p ON p.id = i.project_id AND p.organisation_id = i.organisation_id
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
