-- Lock the organisations table before users, in the order the API takes them (an organisation's
-- row, then its users), so that the foreign keys below, which lock users, never deadlock with a
-- sign-in or an agent registration during a rolling upgrade.
LOCK TABLE "organisations" IN SHARE ROW EXCLUSIVE MODE;--> statement-breakpoint
CREATE TABLE "agent_credentials" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"fingerprint" text NOT NULL,
	"algorithm" text NOT NULL,
	"certificate" text NOT NULL,
	"public_key" text NOT NULL,
	"scopes" text[] NOT NULL,
	"label" text,
	"created_by_id" uuid,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"last_used_at" timestamp with time zone,
	"revoked_at" timestamp with time zone,
	"revoked_by_id" uuid,
	CONSTRAINT "agent_credentials_fingerprint_hex" CHECK ("fingerprint" ~ '^[0-9a-f]{64}$'),
	CONSTRAINT "agent_credentials_algorithm" CHECK ("algorithm" in ('EdDSA', 'ES256')),
	CONSTRAINT "agent_credentials_scopes" CHECK (cardinality("scopes") between 1 and 2 and "scopes" <@ array['api', 'irc']::text[])
);
--> statement-breakpoint
CREATE TABLE "agent_token_ids" (
	"credential_id" uuid,
	"jti" text,
	"expires_at" timestamp with time zone NOT NULL,
	CONSTRAINT "agent_token_ids_pkey" PRIMARY KEY("credential_id","jti")
);
--> statement-breakpoint
CREATE UNIQUE INDEX "agent_credentials_active_fingerprint" ON "agent_credentials" ("fingerprint") WHERE "revoked_at" is null;--> statement-breakpoint
CREATE INDEX "agent_credentials_agent" ON "agent_credentials" ("organisation_id","user_id");--> statement-breakpoint
CREATE INDEX "agent_credentials_fingerprint" ON "agent_credentials" ("organisation_id","fingerprint");--> statement-breakpoint
CREATE INDEX "agent_token_ids_expiry" ON "agent_token_ids" ("expires_at");--> statement-breakpoint
ALTER TABLE "agent_credentials" ADD CONSTRAINT "agent_credentials_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "agent_credentials" ADD CONSTRAINT "agent_credentials_u8BV8STg9dr5_fkey" FOREIGN KEY ("organisation_id","user_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "agent_credentials" ADD CONSTRAINT "agent_credentials_z1fhoPNy1SYa_fkey" FOREIGN KEY ("organisation_id","created_by_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "agent_credentials" ADD CONSTRAINT "agent_credentials_moe6O1Qrdf2C_fkey" FOREIGN KEY ("organisation_id","revoked_by_id") REFERENCES "users"("organisation_id","id");
