ALTER TABLE "issue_comments" ADD COLUMN "resolved_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issue_comments" ADD COLUMN "resolved_by_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_comments" ADD CONSTRAINT "issue_comments_V619K6CTScCM_fkey" FOREIGN KEY ("organisation_id","resolved_by_id") REFERENCES "users"("organisation_id","id");
