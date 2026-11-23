-- Lock the organisations table before users, in the order the API takes them (an organisation's
-- row, then its users, then its fleets and agent credentials), so that the foreign keys below
-- never deadlock with a sign-in or an agent registration during a rolling upgrade.
LOCK TABLE "organisations" IN SHARE ROW EXCLUSIVE MODE;--> statement-breakpoint
CREATE TABLE "enrolment_tokens" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"fleet_id" uuid NOT NULL,
	"token_hash" text NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"max_uses" integer NOT NULL,
	"uses" integer DEFAULT 0 NOT NULL,
	"created_by_id" uuid NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"revoked_at" timestamp with time zone,
	"revoked_by_id" uuid,
	CONSTRAINT "enrolment_tokens_scope_id" UNIQUE("organisation_id","id"),
	CONSTRAINT "enrolment_tokens_hash" CHECK ("token_hash" ~ '^[0-9a-f]{64}$'),
	CONSTRAINT "enrolment_tokens_uses" CHECK ("max_uses" between 1 and 100 and "uses" between 0 and "max_uses")
);--> statement-breakpoint
ALTER TABLE "enrolment_tokens" ADD CONSTRAINT "enrolment_tokens_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "enrolment_tokens" ADD CONSTRAINT "enrolment_tokens_9QiAaHjR4fJT_fkey" FOREIGN KEY ("organisation_id","created_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "enrolment_tokens" ADD CONSTRAINT "enrolment_tokens_liO949CCTWgJ_fkey" FOREIGN KEY ("organisation_id","revoked_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "enrolment_tokens" ADD CONSTRAINT "enrolment_tokens_313ls2QGVxME_fkey" FOREIGN KEY ("organisation_id","fleet_id") REFERENCES "fleets"("organisation_id","id") ON DELETE CASCADE;--> statement-breakpoint
CREATE INDEX "enrolment_tokens_fleet" ON "enrolment_tokens" ("organisation_id","fleet_id");--> statement-breakpoint
ALTER TABLE "agent_credentials" ADD COLUMN "enrolment_token_id" uuid;--> statement-breakpoint
ALTER TABLE "agent_credentials" ADD CONSTRAINT "agent_credentials_IJnypBhr5z4J_fkey" FOREIGN KEY ("organisation_id","enrolment_token_id") REFERENCES "enrolment_tokens"("organisation_id","id");
