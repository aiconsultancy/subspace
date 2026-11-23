CREATE TABLE "retired_usernames" (
	"organisation_id" uuid,
	"name" text,
	"user_id" uuid NOT NULL,
	"retired_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "retired_usernames_pkey" PRIMARY KEY("organisation_id","name"),
	CONSTRAINT "retired_usernames_canonical" CHECK ("name" = lower("name" COLLATE "C"))
);
--> statement-breakpoint
DROP INDEX "user_agent_irc_nick";--> statement-breakpoint
CREATE UNIQUE INDEX "user_agent_irc_nick" ON "users" ("organisation_id",lower("agent_irc_nick" COLLATE "C")) WHERE "agent_irc_nick" is not null;--> statement-breakpoint
DROP INDEX "users_username";--> statement-breakpoint
CREATE UNIQUE INDEX "users_username" ON "users" ("organisation_id",lower("username" COLLATE "C")) WHERE "username" is not null;--> statement-breakpoint
ALTER TABLE "retired_usernames" ADD CONSTRAINT "retired_usernames_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "retired_usernames" ADD CONSTRAINT "retired_usernames_IgBQRpsBHDVC_fkey" FOREIGN KEY ("organisation_id","user_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
-- An issued username that changes or is cleared, such as an agent's IRC nick, is retired, so it
-- is never handed to anyone else: its IRC account may still exist. AFTER UPDATE on every column,
-- so a change a BEFORE trigger makes, such as the agent nick trigger's, is seen too.
CREATE OR REPLACE FUNCTION subspace_retire_username() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO "retired_usernames" ("organisation_id", "name", "user_id")
  VALUES (OLD.organisation_id, lower(OLD.username COLLATE "C"), OLD.id)
  ON CONFLICT ("organisation_id", "name") DO NOTHING;
  RETURN NULL;
END;
$$;
--> statement-breakpoint
CREATE OR REPLACE TRIGGER subspace_users_retire_username
AFTER UPDATE ON "users"
FOR EACH ROW
WHEN (OLD.username IS NOT NULL AND
  lower(OLD.username COLLATE "C") IS DISTINCT FROM lower(NEW.username COLLATE "C"))
EXECUTE FUNCTION subspace_retire_username();
--> statement-breakpoint
-- No record may take a username another record has retired. It fires after the agent nick
-- trigger, as triggers of one kind fire in name order.
CREATE OR REPLACE FUNCTION subspace_username_not_retired() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.username IS NULL THEN
    RETURN NEW;
  END IF;
  IF TG_OP = 'UPDATE' THEN
    IF lower(OLD.username COLLATE "C") IS NOT DISTINCT FROM lower(NEW.username COLLATE "C") THEN
      RETURN NEW;
    END IF;
  END IF;
  IF EXISTS (
    SELECT 1 FROM "retired_usernames"
    WHERE "organisation_id" = NEW.organisation_id
      AND "name" = lower(NEW.username COLLATE "C")
      AND "user_id" <> NEW.id
  ) THEN
    RAISE EXCEPTION 'The username % has been retired in this organisation.', NEW.username
      USING ERRCODE = 'unique_violation', CONSTRAINT = 'retired_usernames_pkey';
  END IF;
  RETURN NEW;
END;
$$;
--> statement-breakpoint
CREATE OR REPLACE TRIGGER subspace_users_username_not_retired
BEFORE INSERT OR UPDATE ON "users"
FOR EACH ROW EXECUTE FUNCTION subspace_username_not_retired();
