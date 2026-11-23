-- Lock the organisations table before users, in the order the API takes them (an organisation's
-- row, then its users), so a sign-in during a rolling upgrade waits rather than deadlocks.
LOCK TABLE "organisations" IN SHARE ROW EXCLUSIVE MODE;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "username" text;--> statement-breakpoint
-- An agent's username is its IRC nick, so people and agents share one namespace. People are
-- backfilled by `bun run db:migrate`, which applies the allocation rule and the operator's
-- reserved names after this migration, oldest first.
UPDATE "users" SET "username" = "agent_irc_nick"
WHERE "agent_rank" IS NOT NULL AND "username" IS DISTINCT FROM "agent_irc_nick";--> statement-breakpoint
CREATE UNIQUE INDEX "users_username" ON "users" ("organisation_id",lower("username")) WHERE "username" is not null;--> statement-breakpoint
-- Keeps an agent's username equal to its IRC nick on every write, including writes from an
-- older API instance during a rolling deployment that sets only the nick.
CREATE OR REPLACE FUNCTION subspace_agent_username() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.agent_rank IS NOT NULL THEN
    NEW.username := NEW.agent_irc_nick;
  END IF;
  RETURN NEW;
END;
$$;
--> statement-breakpoint
CREATE OR REPLACE TRIGGER subspace_users_agent_username
BEFORE INSERT OR UPDATE OF "agent_rank", "agent_irc_nick", "username" ON "users"
FOR EACH ROW EXECUTE FUNCTION subspace_agent_username();
