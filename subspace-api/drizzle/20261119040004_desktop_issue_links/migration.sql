CREATE TABLE "project_decision_issues" (
	"organisation_id" uuid,
	"decision_id" uuid,
	"issue_id" uuid,
	CONSTRAINT "project_decision_issues_pkey" PRIMARY KEY("organisation_id","decision_id","issue_id")
);
--> statement-breakpoint
CREATE TABLE "project_decisions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"project_id" uuid NOT NULL,
	"recorded_by_id" uuid NOT NULL,
	"title" text NOT NULL,
	"decided_on" date NOT NULL,
	"status" text DEFAULT 'proposed' NOT NULL,
	"decision" text NOT NULL,
	"context" text DEFAULT '' NOT NULL,
	"why" text DEFAULT '' NOT NULL,
	"implications" text DEFAULT '' NOT NULL,
	"alternatives" text DEFAULT '' NOT NULL,
	"tags" jsonb DEFAULT '[]' NOT NULL,
	"evidence_kind" text,
	"evidence_reference" text,
	"evidence_excerpt" text,
	"accepted_at" timestamp with time zone,
	"accepted_by_user_id" uuid,
	"rejected_at" timestamp with time zone,
	"rejected_by_user_id" uuid,
	"version" integer DEFAULT 1 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "project_decisions_organisation_id_id_unique" UNIQUE("organisation_id","id")
);
--> statement-breakpoint
CREATE TABLE "issue_identifier_aliases" (
	"organisation_id" uuid,
	"identifier" text,
	"issue_id" uuid NOT NULL,
	CONSTRAINT "issue_identifier_aliases_pkey" PRIMARY KEY("organisation_id","identifier")
);
--> statement-breakpoint
CREATE TABLE "issue_relations" (
	"organisation_id" uuid,
	"source_id" uuid,
	"target_id" uuid,
	"kind" text,
	CONSTRAINT "issue_relations_pkey" PRIMARY KEY("organisation_id","kind","source_id","target_id"),
	CONSTRAINT "issue_relations_distinct" CHECK ("source_id" <> "target_id")
);
--> statement-breakpoint
CREATE TABLE "project_repositories" (
	"organisation_id" uuid,
	"project_id" uuid,
	"repository" text,
	CONSTRAINT "project_repositories_pkey" PRIMARY KEY("organisation_id","project_id","repository")
);
--> statement-breakpoint
CREATE TABLE "issue_pull_request_links" (
	"organisation_id" uuid,
	"issue_id" uuid,
	"pull_request_id" uuid,
	"kind" text NOT NULL,
	"sources" jsonb NOT NULL,
	"active" boolean DEFAULT true NOT NULL,
	CONSTRAINT "issue_pull_request_links_pkey" PRIMARY KEY("organisation_id","issue_id","pull_request_id")
);
--> statement-breakpoint
CREATE TABLE "github_pull_requests" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"repository" text NOT NULL,
	"number" integer NOT NULL,
	"title" text NOT NULL,
	"author_login" text,
	"state" text NOT NULL,
	"is_draft" boolean NOT NULL,
	"head_branch" text NOT NULL,
	"base_branch" text NOT NULL,
	"merged_at" timestamp with time zone,
	"closed_at" timestamp with time zone,
	"github_updated_at" timestamp with time zone NOT NULL,
	"last_observed_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "github_pull_requests_organisation_id_id_unique" UNIQUE("organisation_id","id"),
	CONSTRAINT "github_pull_requests_organisation_id_repository_number_unique" UNIQUE("organisation_id","repository","number")
);
--> statement-breakpoint
CREATE INDEX "project_decision_issues_organisation_id_issue_id_index" ON "project_decision_issues" ("organisation_id","issue_id");--> statement-breakpoint
CREATE INDEX "project_decisions_organisation_id_project_id_decided_on_index" ON "project_decisions" ("organisation_id","project_id","decided_on");--> statement-breakpoint
CREATE INDEX "issue_relations_organisation_id_target_id_index" ON "issue_relations" ("organisation_id","target_id");--> statement-breakpoint
CREATE INDEX "project_repositories_repository_index" ON "project_repositories" ("repository");--> statement-breakpoint
CREATE INDEX "issue_pull_request_links_organisation_id_pull_request_id_index" ON "issue_pull_request_links" ("organisation_id","pull_request_id");--> statement-breakpoint
ALTER TABLE "project_decision_issues" ADD CONSTRAINT "project_decision_issues_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_decision_issues" ADD CONSTRAINT "project_decision_issues_pC92h07rTqAS_fkey" FOREIGN KEY ("organisation_id","decision_id") REFERENCES "project_decisions"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_decision_issues" ADD CONSTRAINT "project_decision_issues_x9L083BvAv5s_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "project_decisions" ADD CONSTRAINT "project_decisions_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_decisions" ADD CONSTRAINT "project_decisions_hrVWkxmRK0WO_fkey" FOREIGN KEY ("organisation_id","project_id") REFERENCES "projects"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "project_decisions" ADD CONSTRAINT "project_decisions_u5OHgJuCtNg1_fkey" FOREIGN KEY ("organisation_id","recorded_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_identifier_aliases" ADD CONSTRAINT "issue_identifier_aliases_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_identifier_aliases" ADD CONSTRAINT "issue_identifier_aliases_R48KDyyer2Q3_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_relations" ADD CONSTRAINT "issue_relations_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_relations" ADD CONSTRAINT "issue_relations_tOtUgDWSuy0U_fkey" FOREIGN KEY ("organisation_id","source_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_relations" ADD CONSTRAINT "issue_relations_wFONTWAjBKl8_fkey" FOREIGN KEY ("organisation_id","target_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_repositories" ADD CONSTRAINT "project_repositories_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_repositories" ADD CONSTRAINT "project_repositories_2ObWA3gy9GqN_fkey" FOREIGN KEY ("organisation_id","project_id") REFERENCES "projects"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_pull_request_links" ADD CONSTRAINT "issue_pull_request_links_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_pull_request_links" ADD CONSTRAINT "issue_pull_request_links_1NY4gsgCt6v2_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_pull_request_links" ADD CONSTRAINT "issue_pull_request_links_8ygU1kVT3h8G_fkey" FOREIGN KEY ("organisation_id","pull_request_id") REFERENCES "github_pull_requests"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "github_pull_requests" ADD CONSTRAINT "github_pull_requests_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;
