CREATE TABLE "issue_todos" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"issue_id" uuid NOT NULL,
	"text" text NOT NULL,
	"position" integer NOT NULL,
	"done_at" timestamp with time zone,
	"done_by_id" uuid,
	"created_by_id" uuid NOT NULL,
	"updated_by_id" uuid,
	"version" integer DEFAULT 1 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "issue_todos_organisation_id_issue_id_position_unique" UNIQUE("organisation_id","issue_id","position")
);
--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "archived_by_id" uuid;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_JskMR9b2U8WQ_fkey" FOREIGN KEY ("organisation_id","archived_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_todos" ADD CONSTRAINT "issue_todos_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_todos" ADD CONSTRAINT "issue_todos_LFrZ2CJvbiW8_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_todos" ADD CONSTRAINT "issue_todos_P7Jlab01fqL5_fkey" FOREIGN KEY ("organisation_id","created_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_todos" ADD CONSTRAINT "issue_todos_0XBVA0J88hvw_fkey" FOREIGN KEY ("organisation_id","updated_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_todos" ADD CONSTRAINT "issue_todos_Ct7wvyUu4Akt_fkey" FOREIGN KEY ("organisation_id","done_by_id") REFERENCES "users"("organisation_id","id");
