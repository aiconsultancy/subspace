-- Lock the organisations table first, in the order the API takes them (an organisation's row,
-- then its users, then its agent credentials), so that this migration never deadlocks with a
-- sign-in, a pairing or a link during a rolling upgrade.
LOCK TABLE "organisations" IN SHARE ROW EXCLUSIVE MODE;--> statement-breakpoint
CREATE TABLE "member_identities" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"provider" text NOT NULL,
	"external_organisation" text NOT NULL,
	"subject" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "member_identities_scope_id" UNIQUE("organisation_id","id"),
	CONSTRAINT "member_identities_account" UNIQUE("provider","external_organisation","subject"),
	CONSTRAINT "member_identities_one_per_provider" UNIQUE("organisation_id","user_id","provider"),
	CONSTRAINT "member_identities_provider" CHECK ("provider" in ('workos', 'entra'))
);
--> statement-breakpoint
ALTER TABLE "member_identities" ADD CONSTRAINT "member_identities_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "member_identities" ADD CONSTRAINT "member_identities_p3cJIt8Y6fjS_fkey" FOREIGN KEY ("organisation_id","user_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
-- A paired device may now ask for an IRC password, bound to its credential.
UPDATE "agent_credentials" SET "scopes" = array['api', 'irc']::text[] WHERE "pairing_invite_id" IS NOT NULL;
