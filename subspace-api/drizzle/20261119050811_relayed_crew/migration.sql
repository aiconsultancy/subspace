ALTER TABLE "issue_comments" ADD COLUMN "relayed_crew" jsonb;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "relayed_crew" jsonb;
