CREATE TABLE "uploads" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"issue_id" uuid,
	"client_id" uuid,
	"uploaded_by_id" uuid NOT NULL,
	"kind" text NOT NULL,
	"state" text DEFAULT 'pending' NOT NULL,
	"file_name" text NOT NULL,
	"content_type" text NOT NULL,
	"size_bytes" integer NOT NULL,
	"sha256" text NOT NULL,
	"expires_at" timestamp with time zone,
	"purged_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "uploads_organisation_id_client_id_unique" UNIQUE("organisation_id","client_id"),
	CONSTRAINT "upload_ownership" CHECK ((
    ("kind" = 'chat' and "issue_id" is null and "client_id" is not null and "expires_at" is not null)
    or
    ("kind" in ('image', 'file') and "issue_id" is not null and "client_id" is null and "expires_at" is null)
  )),
	CONSTRAINT "upload_size" CHECK ("size_bytes" between 1 and 25000000)
);
--> statement-breakpoint
CREATE INDEX "uploads_organisation_id_issue_id_kind_index" ON "uploads" ("organisation_id","issue_id","kind");--> statement-breakpoint
ALTER TABLE "uploads" ADD CONSTRAINT "uploads_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "uploads" ADD CONSTRAINT "uploads_organisation_id_issue_id_issues_organisation_id_id_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "uploads" ADD CONSTRAINT "uploads_iGiv5MGkEFu8_fkey" FOREIGN KEY ("organisation_id","uploaded_by_id") REFERENCES "users"("organisation_id","id");
