CREATE TABLE "project_decision_revisions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"decision_id" uuid NOT NULL,
	"number" integer NOT NULL,
	"snapshot" jsonb NOT NULL,
	"saved_at" timestamp with time zone DEFAULT now() NOT NULL,
	"saved_by" jsonb NOT NULL,
	"changed_fields" jsonb NOT NULL,
	CONSTRAINT "project_decision_revisions_organisation_id_decision_id_number_unique" UNIQUE("organisation_id","decision_id","number")
);
--> statement-breakpoint
ALTER TABLE "project_decisions" ADD COLUMN "supersedes_id" uuid;--> statement-breakpoint
ALTER TABLE "project_decisions" ADD COLUMN "superseded_by_id" uuid;--> statement-breakpoint
ALTER TABLE "project_decision_revisions" ADD CONSTRAINT "project_decision_revisions_HbKHxDgJ777y_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_decision_revisions" ADD CONSTRAINT "project_decision_revisions_JLY8QLovbCpW_fkey" FOREIGN KEY ("organisation_id","decision_id") REFERENCES "project_decisions"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_decisions" ADD CONSTRAINT "project_decisions_NjzNFb5kCJK3_fkey" FOREIGN KEY ("organisation_id","supersedes_id") REFERENCES "project_decisions"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "project_decisions" ADD CONSTRAINT "project_decisions_gFeFr8udgMMu_fkey" FOREIGN KEY ("organisation_id","superseded_by_id") REFERENCES "project_decisions"("organisation_id","id");
