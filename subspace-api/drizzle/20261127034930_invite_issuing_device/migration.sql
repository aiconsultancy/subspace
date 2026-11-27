-- Lock the organisations table first, in the order the API takes them (an organisation's row,
-- then its agent credentials and invites), so that this migration never deadlocks with a
-- pairing, an invite or a revocation during a rolling upgrade.
LOCK TABLE "organisations" IN SHARE ROW EXCLUSIVE MODE;--> statement-breakpoint
-- The device credential that issued an invite, so that revoking it reaches what it paired.
ALTER TABLE "pairing_invites" ADD COLUMN "created_by_credential_id" uuid;--> statement-breakpoint
ALTER TABLE "agent_credentials" ADD CONSTRAINT "agent_credentials_scope_id" UNIQUE("organisation_id","id");--> statement-breakpoint
CREATE INDEX "pairing_invites_issuing_device" ON "pairing_invites" ("created_by_credential_id");--> statement-breakpoint
ALTER TABLE "pairing_invites" ADD CONSTRAINT "pairing_invites_O1nHHzjDzCVr_fkey" FOREIGN KEY ("organisation_id","created_by_credential_id") REFERENCES "agent_credentials"("organisation_id","id");
