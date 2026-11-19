CREATE TABLE "project_document_counters" (
	"organisation_id" uuid,
	"project_id" uuid,
	"number" integer NOT NULL,
	CONSTRAINT "project_document_counters_pkey" PRIMARY KEY("organisation_id","project_id")
);
--> statement-breakpoint
CREATE TABLE "issue_document_revisions" (
	"organisation_id" uuid,
	"document_id" uuid,
	"number" integer,
	"title" text NOT NULL,
	"content" text NOT NULL,
	"note" text,
	"author_id" uuid NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "issue_document_revisions_pkey" PRIMARY KEY("organisation_id","document_id","number")
);
--> statement-breakpoint
CREATE TABLE "issue_documents" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"issue_id" uuid NOT NULL,
	"original_project_id" uuid NOT NULL,
	"reference" text NOT NULL,
	"type" text NOT NULL,
	"subtype" text,
	"author_id" uuid NOT NULL,
	"current_revision" integer DEFAULT 1 NOT NULL,
	"archived_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "issue_documents_organisation_id_id_unique" UNIQUE("organisation_id","id"),
	CONSTRAINT "issue_documents_organisation_id_reference_unique" UNIQUE("organisation_id","reference")
);
--> statement-breakpoint
CREATE TABLE "issue_reviews" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"issue_id" uuid NOT NULL,
	"kind" text NOT NULL,
	"model" text NOT NULL,
	"reasoning" text,
	"verdict" text NOT NULL,
	"subject" text,
	"revision" text,
	"document_id" uuid,
	"document_revision" integer,
	"notes" text,
	"recorded_by_id" uuid NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "review_document_revision" CHECK (("document_id" is null) = ("document_revision" is null))
);
--> statement-breakpoint
CREATE TABLE "project_review_requirements" (
	"organisation_id" uuid,
	"project_id" uuid,
	"kinds" jsonb NOT NULL,
	CONSTRAINT "project_review_requirements_pkey" PRIMARY KEY("organisation_id","project_id")
);
--> statement-breakpoint
CREATE INDEX "issue_documents_organisation_id_issue_id_index" ON "issue_documents" ("organisation_id","issue_id");--> statement-breakpoint
CREATE INDEX "issue_reviews_organisation_id_issue_id_created_at_index" ON "issue_reviews" ("organisation_id","issue_id","created_at");--> statement-breakpoint
ALTER TABLE "project_document_counters" ADD CONSTRAINT "project_document_counters_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_document_counters" ADD CONSTRAINT "project_document_counters_Cp7UcIyp6uay_fkey" FOREIGN KEY ("organisation_id","project_id") REFERENCES "projects"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_document_revisions" ADD CONSTRAINT "issue_document_revisions_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_document_revisions" ADD CONSTRAINT "issue_document_revisions_OafB9xCSBB0j_fkey" FOREIGN KEY ("organisation_id","document_id") REFERENCES "issue_documents"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_document_revisions" ADD CONSTRAINT "issue_document_revisions_4xyHBFqes3RR_fkey" FOREIGN KEY ("organisation_id","author_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_documents" ADD CONSTRAINT "issue_documents_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_documents" ADD CONSTRAINT "issue_documents_FhvK68IYJKhb_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_documents" ADD CONSTRAINT "issue_documents_Hz6PMCQBG1L3_fkey" FOREIGN KEY ("organisation_id","original_project_id") REFERENCES "projects"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_documents" ADD CONSTRAINT "issue_documents_uVGVdG1VBtpn_fkey" FOREIGN KEY ("organisation_id","author_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_reviews" ADD CONSTRAINT "issue_reviews_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_reviews" ADD CONSTRAINT "issue_reviews_IrpN5NdBIwm8_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_reviews" ADD CONSTRAINT "issue_reviews_3Zcn6SVFof8X_fkey" FOREIGN KEY ("organisation_id","recorded_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_reviews" ADD CONSTRAINT "issue_reviews_kbj1XY76mGHW_fkey" FOREIGN KEY ("organisation_id","document_id","document_revision") REFERENCES "issue_document_revisions"("organisation_id","document_id","number") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_review_requirements" ADD CONSTRAINT "project_review_requirements_XhXJMgdsPOpE_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_review_requirements" ADD CONSTRAINT "project_review_requirements_mBkaurhxmbQu_fkey" FOREIGN KEY ("organisation_id","project_id") REFERENCES "projects"("organisation_id","id") ON DELETE CASCADE;
