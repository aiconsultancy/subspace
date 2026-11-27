-- Lock the organisations table first and create the foreign keys to users before altering
-- agent_credentials, in the order the API takes them (an organisation's row, then its users, then
-- its agent credentials), so that this migration never deadlocks with a sign-in, an agent
-- registration or a pairing during a rolling upgrade.
LOCK TABLE "organisations" IN SHARE ROW EXCLUSIVE MODE;--> statement-breakpoint
CREATE TABLE "pairing_invites" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"token_hash" text NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"created_by_id" uuid,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"redeemed_at" timestamp with time zone,
	"revoked_at" timestamp with time zone,
	CONSTRAINT "pairing_invites_scope_id" UNIQUE("organisation_id","id"),
	CONSTRAINT "pairing_invites_hash" CHECK ("token_hash" ~ '^[0-9a-f]{64}$')
);
--> statement-breakpoint
ALTER TABLE "pairing_invites" ADD CONSTRAINT "pairing_invites_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "pairing_invites" ADD CONSTRAINT "pairing_invites_pC9fSHAAllgW_fkey" FOREIGN KEY ("organisation_id","user_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "pairing_invites" ADD CONSTRAINT "pairing_invites_xfK6HULueFPT_fkey" FOREIGN KEY ("organisation_id","created_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
CREATE INDEX "pairing_invites_member" ON "pairing_invites" ("organisation_id","user_id");--> statement-breakpoint
ALTER TABLE "agent_credentials" ADD COLUMN "pairing_invite_id" uuid;--> statement-breakpoint
CREATE UNIQUE INDEX "agent_credentials_pairing_invite" ON "agent_credentials" ("pairing_invite_id");--> statement-breakpoint
ALTER TABLE "agent_credentials" ADD CONSTRAINT "agent_credentials_ldLnTx6w2SPH_fkey" FOREIGN KEY ("organisation_id","pairing_invite_id") REFERENCES "pairing_invites"("organisation_id","id");
