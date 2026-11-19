CREATE TABLE "fleets" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"slug" text NOT NULL,
	"name" text NOT NULL,
	"irc_host" text NOT NULL,
	"irc_port" integer NOT NULL,
	"irc_tls" boolean NOT NULL,
	"irc_websocket_url" text,
	"rear_admiral_name" text NOT NULL,
	"captain_names" jsonb NOT NULL,
	"vm_host" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "fleets_organisation_slug" UNIQUE("organisation_id","slug"),
	CONSTRAINT "fleets_organisation_id_id_unique" UNIQUE("organisation_id","id"),
	CONSTRAINT "fleets_irc_port" CHECK ("irc_port" between 1 and 65535),
	CONSTRAINT "fleets_captain_names" CHECK (jsonb_typeof("captain_names") = 'array')
);
--> statement-breakpoint
CREATE TABLE "initiative_memberships" (
	"organisation_id" uuid,
	"initiative_id" uuid,
	"user_id" uuid,
	CONSTRAINT "initiative_memberships_pkey" PRIMARY KEY("organisation_id","initiative_id","user_id")
);
--> statement-breakpoint
CREATE TABLE "initiatives" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"name" text NOT NULL,
	"channels" text DEFAULT '' NOT NULL,
	"description" text,
	"owner_id" uuid NOT NULL,
	"parent_id" uuid,
	"fleet_id" uuid,
	"version" integer DEFAULT 1 NOT NULL,
	"status" text DEFAULT 'planned' NOT NULL,
	"target_date" date,
	"archived" boolean DEFAULT false NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "initiatives_organisation_id_id_unique" UNIQUE("organisation_id","id"),
	CONSTRAINT "initiatives_status" CHECK ("status" in ('planned', 'active', 'completed', 'canceled')),
	CONSTRAINT "initiatives_version" CHECK ("version" > 0)
);
--> statement-breakpoint
CREATE TABLE "organisations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"provider" text NOT NULL,
	"external_id" text NOT NULL,
	CONSTRAINT "organisations_provider_external_id_unique" UNIQUE("provider","external_id")
);
--> statement-breakpoint
CREATE TABLE "users" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"subject" text NOT NULL,
	"email" text,
	"display_name" text NOT NULL,
	"role" text DEFAULT 'member' NOT NULL,
	"active" boolean DEFAULT true NOT NULL,
	CONSTRAINT "users_organisation_id_subject_unique" UNIQUE("organisation_id","subject"),
	CONSTRAINT "users_organisation_id_id_unique" UNIQUE("organisation_id","id"),
	CONSTRAINT "users_role" CHECK ("role" in ('admin', 'member'))
);
--> statement-breakpoint
CREATE INDEX "initiative_memberships_organisation_id_user_id_index" ON "initiative_memberships" ("organisation_id","user_id");--> statement-breakpoint
ALTER TABLE "fleets" ADD CONSTRAINT "fleets_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "initiative_memberships" ADD CONSTRAINT "initiative_memberships_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "initiative_memberships" ADD CONSTRAINT "initiative_memberships_Hq53YY1PCzOJ_fkey" FOREIGN KEY ("organisation_id","initiative_id") REFERENCES "initiatives"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "initiative_memberships" ADD CONSTRAINT "initiative_memberships_WUWIgZUzmRin_fkey" FOREIGN KEY ("organisation_id","user_id") REFERENCES "users"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "initiatives" ADD CONSTRAINT "initiatives_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "initiatives" ADD CONSTRAINT "initiatives_OVRK26iEq1Ko_fkey" FOREIGN KEY ("organisation_id","owner_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "initiatives" ADD CONSTRAINT "initiatives_oEAymN28lN0W_fkey" FOREIGN KEY ("organisation_id","fleet_id") REFERENCES "fleets"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "initiatives" ADD CONSTRAINT "initiatives_IJpJ0m73SB1s_fkey" FOREIGN KEY ("organisation_id","parent_id") REFERENCES "initiatives"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "users" ADD CONSTRAINT "users_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;
