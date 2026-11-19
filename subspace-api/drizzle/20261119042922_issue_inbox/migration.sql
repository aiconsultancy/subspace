CREATE TABLE "issue_notifications" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"issue_id" uuid NOT NULL,
	"project_id" uuid NOT NULL,
	"comment_id" uuid,
	"actor_user_id" uuid,
	"change_id" uuid NOT NULL,
	"type" text NOT NULL,
	"title" text NOT NULL,
	"body" text NOT NULL,
	"read_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "issue_notifications_organisation_id_user_id_change_id_unique" UNIQUE("organisation_id","user_id","change_id")
);
--> statement-breakpoint
CREATE TABLE "issue_subscriptions" (
	"organisation_id" uuid,
	"issue_id" uuid,
	"user_id" uuid,
	"subscribed" boolean DEFAULT true NOT NULL,
	"explicit_choice" boolean DEFAULT false NOT NULL,
	CONSTRAINT "issue_subscriptions_pkey" PRIMARY KEY("organisation_id","issue_id","user_id")
);
--> statement-breakpoint
CREATE INDEX "issue_notifications_organisation_id_user_id_created_at_id_index" ON "issue_notifications" ("organisation_id","user_id","created_at","id");--> statement-breakpoint
ALTER TABLE "issue_notifications" ADD CONSTRAINT "issue_notifications_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_notifications" ADD CONSTRAINT "issue_notifications_t5jg0ekAB5it_fkey" FOREIGN KEY ("organisation_id","user_id") REFERENCES "users"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_notifications" ADD CONSTRAINT "issue_notifications_CWS9R8a55RUR_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_subscriptions" ADD CONSTRAINT "issue_subscriptions_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_subscriptions" ADD CONSTRAINT "issue_subscriptions_CvbPaoegbqCP_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_subscriptions" ADD CONSTRAINT "issue_subscriptions_sDCVjuoLGE0r_fkey" FOREIGN KEY ("organisation_id","user_id") REFERENCES "users"("organisation_id","id") ON DELETE CASCADE;
--> statement-breakpoint
CREATE FUNCTION subspace_issue_inbox() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  issue_row issues%ROWTYPE;
  comment_id uuid;
  notice_type text;
  actor_name text;
BEGIN
  IF NEW.kind NOT IN ('issue.created', 'issue.changed', 'issue.state-derived', 'comment.created', 'comment.edited') THEN
    RETURN NEW;
  END IF;

  SELECT * INTO issue_row FROM issues
  WHERE id = NEW.issue_id AND organisation_id = NEW.organisation_id;

  INSERT INTO issue_subscriptions (organisation_id, issue_id, user_id)
  SELECT DISTINCT NEW.organisation_id, NEW.issue_id, u.id
  FROM users u
  WHERE u.organisation_id = NEW.organisation_id AND u.active AND (
    (NEW.kind IN ('issue.created', 'comment.created') AND u.id = NEW.actor_id)
    OR (NEW.kind = 'issue.created' AND u.id = issue_row.assignee_id)
    OR (NEW.kind = 'issue.changed' AND (NEW.data->'fields') ? 'assigneeId' AND u.id = issue_row.assignee_id)
  )
  ON CONFLICT DO NOTHING;

  comment_id := (NEW.data->>'commentId')::uuid;
  notice_type := CASE
    WHEN NEW.kind = 'comment.created' THEN 'issueCommentAdded'
    WHEN NEW.kind = 'issue.state-derived' OR NEW.data ? 'previousStateId' OR NEW.source = 'github' THEN 'issueStateChanged'
    ELSE NULL
  END;
  SELECT display_name INTO actor_name FROM users
  WHERE organisation_id = NEW.organisation_id AND id = NEW.actor_id;
  actor_name := coalesce(actor_name, 'GitHub');

  WITH candidates AS (
    SELECT value::uuid AS user_id, true AS mentioned
    FROM jsonb_array_elements_text(coalesce(NEW.data->'mentionUserIds', '[]'::jsonb))
    UNION ALL
    SELECT s.user_id, false
    FROM issue_subscriptions s
    WHERE notice_type IS NOT NULL AND s.organisation_id = NEW.organisation_id
      AND s.issue_id = NEW.issue_id AND s.subscribed
  ), recipients AS (
    SELECT c.user_id, bool_or(c.mentioned) AS mentioned
    FROM candidates c JOIN users u ON u.id = c.user_id AND u.organisation_id = NEW.organisation_id AND u.active
    WHERE c.user_id IS DISTINCT FROM NEW.actor_id
    GROUP BY c.user_id
  )
  INSERT INTO issue_notifications (organisation_id, user_id, issue_id, project_id,
    comment_id, actor_user_id, change_id, type, title, body)
  SELECT NEW.organisation_id, r.user_id, NEW.issue_id, issue_row.project_id, comment_id, NEW.actor_id, NEW.id,
    CASE WHEN r.mentioned THEN 'mentioned' ELSE notice_type END, issue_row.title,
    actor_name || CASE
      WHEN r.mentioned THEN ' mentioned you.'
      WHEN notice_type = 'issueCommentAdded' THEN ' commented on an issue you follow.'
      ELSE ' changed the state of an issue you follow.'
    END
  FROM recipients r
  ON CONFLICT DO NOTHING;
  RETURN NEW;
END;
$$;
--> statement-breakpoint
CREATE TRIGGER subspace_issue_activity_inbox
AFTER INSERT ON issue_activity FOR EACH ROW EXECUTE FUNCTION subspace_issue_inbox();
