CREATE TABLE "project_updates" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"project_id" uuid NOT NULL,
	"author_id" uuid NOT NULL,
	"body" text NOT NULL,
	"health" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"edited_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "project_milestones" ADD COLUMN "sort_order" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "project_milestones" ADD COLUMN "completed_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "project_milestones" ADD COLUMN "completed_by_id" uuid;--> statement-breakpoint
ALTER TABLE "project_milestones" ADD COLUMN "version" integer DEFAULT 1 NOT NULL;--> statement-breakpoint
ALTER TABLE "project_milestones" ADD COLUMN "updated_at" timestamp with time zone DEFAULT now() NOT NULL;--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "health" text;--> statement-breakpoint
CREATE INDEX "project_updates_organisation_id_project_id_created_at_id_index" ON "project_updates" ("organisation_id","project_id","created_at","id");--> statement-breakpoint
ALTER TABLE "project_milestones" ADD CONSTRAINT "project_milestones_ZkjOQWwxhHWs_fkey" FOREIGN KEY ("organisation_id","completed_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "project_updates" ADD CONSTRAINT "project_updates_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_updates" ADD CONSTRAINT "project_updates_WHs7oSbzljy7_fkey" FOREIGN KEY ("organisation_id","project_id") REFERENCES "projects"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_updates" ADD CONSTRAINT "project_updates_lvXgV47zNRaX_fkey" FOREIGN KEY ("organisation_id","author_id") REFERENCES "users"("organisation_id","id");
