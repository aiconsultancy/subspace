CREATE TABLE "issue_comments" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"issue_id" uuid NOT NULL,
	"parent_comment_id" uuid,
	"author_id" uuid NOT NULL,
	"body" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"edited_at" timestamp with time zone,
	"deleted_at" timestamp with time zone,
	CONSTRAINT "issue_comments_organisation_id_issue_id_id_unique" UNIQUE("organisation_id","issue_id","id")
);
--> statement-breakpoint
CREATE TABLE "issue_cycles" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"project_id" uuid NOT NULL,
	"number" integer NOT NULL,
	"state" text NOT NULL,
	"start_date" date NOT NULL,
	"end_date" date NOT NULL,
	CONSTRAINT "issue_cycles_organisation_id_project_id_id_unique" UNIQUE("organisation_id","project_id","id"),
	CONSTRAINT "issue_cycles_organisation_id_project_id_number_unique" UNIQUE("organisation_id","project_id","number")
);
--> statement-breakpoint
CREATE TABLE "issue_activity" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"issue_id" uuid NOT NULL,
	"actor_id" uuid NOT NULL,
	"kind" text NOT NULL,
	"data" jsonb NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "issue_labels" (
	"organisation_id" uuid,
	"issue_id" uuid,
	"label_id" uuid,
	CONSTRAINT "issue_labels_pkey" PRIMARY KEY("organisation_id","issue_id","label_id")
);
--> statement-breakpoint
CREATE TABLE "issues" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"project_id" uuid NOT NULL,
	"number" integer NOT NULL,
	"identifier" text NOT NULL,
	"type" text DEFAULT 'issue' NOT NULL,
	"title" text NOT NULL,
	"summary" text NOT NULL,
	"description" text,
	"state_id" uuid NOT NULL,
	"assignee_id" uuid,
	"owner_id" uuid,
	"created_by_id" uuid NOT NULL,
	"parent_id" uuid,
	"milestone_id" uuid,
	"cycle_id" uuid,
	"priority" integer DEFAULT 0 NOT NULL,
	"estimate" double precision,
	"due_date" date,
	"version" integer DEFAULT 1 NOT NULL,
	"archived_at" timestamp with time zone,
	"started_at" timestamp with time zone,
	"completed_at" timestamp with time zone,
	"canceled_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	"last_activity_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "issues_organisation_id_id_unique" UNIQUE("organisation_id","id"),
	CONSTRAINT "issues_organisation_id_identifier_unique" UNIQUE("organisation_id","identifier"),
	CONSTRAINT "issue_priority" CHECK ("priority" between 0 and 4)
);
--> statement-breakpoint
CREATE TABLE "labels" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"name" text NOT NULL,
	"color" text NOT NULL,
	"state" text DEFAULT 'active' NOT NULL,
	CONSTRAINT "labels_organisation_id_id_unique" UNIQUE("organisation_id","id"),
	CONSTRAINT "labels_organisation_id_name_unique" UNIQUE("organisation_id","name")
);
--> statement-breakpoint
CREATE TABLE "project_milestones" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"project_id" uuid NOT NULL,
	"name" text NOT NULL,
	"description" text,
	"target_date" date,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "project_milestones_organisation_id_project_id_id_unique" UNIQUE("organisation_id","project_id","id")
);
--> statement-breakpoint
CREATE TABLE "projects" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"key" text NOT NULL,
	"name" text NOT NULL,
	"description" text,
	"lead_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"status" text DEFAULT 'backlog' NOT NULL,
	"priority" integer DEFAULT 0 NOT NULL,
	"start_date" date,
	"target_date" date,
	"initiative_id" uuid,
	"default_state_id" uuid NOT NULL,
	"archived_at" timestamp with time zone,
	"archived_by_user_id" uuid,
	"version" integer DEFAULT 1 NOT NULL,
	"next_issue_number" integer DEFAULT 1 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "projects_organisation_id_id_unique" UNIQUE("organisation_id","id"),
	CONSTRAINT "projects_organisation_id_key_unique" UNIQUE("organisation_id","key")
);
--> statement-breakpoint
CREATE TABLE "workflow_states" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"project_id" uuid NOT NULL,
	"name" text NOT NULL,
	"type" text NOT NULL,
	"position" integer NOT NULL,
	"is_default" boolean DEFAULT false NOT NULL,
	CONSTRAINT "workflow_states_organisation_id_project_id_id_unique" UNIQUE("organisation_id","project_id","id"),
	CONSTRAINT "workflow_states_organisation_id_project_id_type_position_unique" UNIQUE("organisation_id","project_id","type","position")
);
--> statement-breakpoint
CREATE INDEX "issue_activity_organisation_id_issue_id_created_at_index" ON "issue_activity" ("organisation_id","issue_id","created_at");--> statement-breakpoint
CREATE INDEX "issues_organisation_id_project_id_index" ON "issues" ("organisation_id","project_id");--> statement-breakpoint
ALTER TABLE "issue_comments" ADD CONSTRAINT "issue_comments_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_comments" ADD CONSTRAINT "issue_comments_yHBZOWLq3I1z_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_comments" ADD CONSTRAINT "issue_comments_XEFJSdB82h5k_fkey" FOREIGN KEY ("organisation_id","issue_id","parent_comment_id") REFERENCES "issue_comments"("organisation_id","issue_id","id");--> statement-breakpoint
ALTER TABLE "issue_comments" ADD CONSTRAINT "issue_comments_7BlPvcBfthYd_fkey" FOREIGN KEY ("organisation_id","author_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_cycles" ADD CONSTRAINT "issue_cycles_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_cycles" ADD CONSTRAINT "issue_cycles_z7xS9sZuZB1A_fkey" FOREIGN KEY ("organisation_id","project_id") REFERENCES "projects"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_activity" ADD CONSTRAINT "issue_activity_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_activity" ADD CONSTRAINT "issue_activity_XZKUy7FhPsfq_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_activity" ADD CONSTRAINT "issue_activity_YukOPnFwfGTc_fkey" FOREIGN KEY ("organisation_id","actor_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issue_labels" ADD CONSTRAINT "issue_labels_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_labels" ADD CONSTRAINT "issue_labels_KqiBAxG4uDDo_fkey" FOREIGN KEY ("organisation_id","issue_id") REFERENCES "issues"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issue_labels" ADD CONSTRAINT "issue_labels_Lj1z9Q2Lzq40_fkey" FOREIGN KEY ("organisation_id","label_id") REFERENCES "labels"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_T8sUxXAwClp5_fkey" FOREIGN KEY ("organisation_id","project_id") REFERENCES "projects"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_organisation_id_parent_id_issues_organisation_id_id_fkey" FOREIGN KEY ("organisation_id","parent_id") REFERENCES "issues"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_oC65RVcbm2gV_fkey" FOREIGN KEY ("organisation_id","project_id","state_id") REFERENCES "workflow_states"("organisation_id","project_id","id");--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_uOLKfZIy3YAL_fkey" FOREIGN KEY ("organisation_id","project_id","milestone_id") REFERENCES "project_milestones"("organisation_id","project_id","id");--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_OEPdDHabC3TT_fkey" FOREIGN KEY ("organisation_id","project_id","cycle_id") REFERENCES "issue_cycles"("organisation_id","project_id","id");--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_dF1ltgNdcpFJ_fkey" FOREIGN KEY ("organisation_id","assignee_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_organisation_id_owner_id_users_organisation_id_id_fkey" FOREIGN KEY ("organisation_id","owner_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_hXreMkhRfRqa_fkey" FOREIGN KEY ("organisation_id","created_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "labels" ADD CONSTRAINT "labels_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_milestones" ADD CONSTRAINT "project_milestones_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "project_milestones" ADD CONSTRAINT "project_milestones_VBtZVqg8l0XW_fkey" FOREIGN KEY ("organisation_id","project_id") REFERENCES "projects"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "projects" ADD CONSTRAINT "projects_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "projects" ADD CONSTRAINT "projects_qlQFqV8pXOUv_fkey" FOREIGN KEY ("organisation_id","initiative_id") REFERENCES "initiatives"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "projects" ADD CONSTRAINT "projects_organisation_id_lead_id_users_organisation_id_id_fkey" FOREIGN KEY ("organisation_id","lead_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "projects" ADD CONSTRAINT "projects_WYLBLgOsHsFQ_fkey" FOREIGN KEY ("organisation_id","created_by_user_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "workflow_states" ADD CONSTRAINT "workflow_states_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "workflow_states" ADD CONSTRAINT "workflow_states_WKNsnuu6mIEF_fkey" FOREIGN KEY ("organisation_id","project_id") REFERENCES "projects"("organisation_id","id");
