CREATE TABLE "issue_views" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"owner_id" uuid NOT NULL,
	"name" text NOT NULL,
	"visibility" text NOT NULL,
	"definition" jsonb NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "issue_views_owner_name" ON "issue_views" ("organisation_id","owner_id",lower("name"));--> statement-breakpoint
CREATE INDEX "issue_views_organisation_id_visibility_index" ON "issue_views" ("organisation_id","visibility");--> statement-breakpoint
ALTER TABLE "issue_views" ADD CONSTRAINT "issue_views_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_views" ADD CONSTRAINT "issue_views_OVW3dCtmNeup_fkey" FOREIGN KEY ("organisation_id","owner_id") REFERENCES "users"("organisation_id","id") ON DELETE CASCADE;
