CREATE TABLE "issue_cycle_results" (
	"organisation_id" uuid,
	"project_id" uuid NOT NULL,
	"cycle_id" uuid,
	"issue_id" uuid,
	"state_type" text NOT NULL,
	"state_name" text NOT NULL,
	"points" double precision,
	"outcome" text NOT NULL,
	"joined_after_start" boolean DEFAULT false NOT NULL,
	CONSTRAINT "issue_cycle_results_pkey" PRIMARY KEY("organisation_id","cycle_id","issue_id")
);
--> statement-breakpoint
ALTER TABLE "issue_cycles" ADD COLUMN "finished_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issue_cycles" ADD COLUMN "finished_early" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_cycles" ADD COLUMN "carried_to_number" integer;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "cycle_joined_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "last_cycle_number" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "cycle_settings" jsonb DEFAULT '{"enabled":false,"lengthWeeks":2,"startDay":"monday","cyclesAhead":2,"carryOver":true}' NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_cycle_results" ADD CONSTRAINT "issue_cycle_results_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_cycle_results" ADD CONSTRAINT "issue_cycle_results_NQ39Mdvhd4Ds_fkey" FOREIGN KEY ("organisation_id","project_id","cycle_id") REFERENCES "issue_cycles"("organisation_id","project_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_cycle_results" ADD CONSTRAINT "issue_cycle_results_bTAvMctFAY1S_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;
