ALTER TABLE "fleets" ADD COLUMN "starbase_channel" text;--> statement-breakpoint
ALTER TABLE "fleets" ADD COLUMN "command_channel" text;--> statement-breakpoint
ALTER TABLE "ships" ADD COLUMN "channel" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "agent_irc_nick" text;--> statement-breakpoint
CREATE UNIQUE INDEX "user_agent_irc_nick" ON "users" ("organisation_id",lower("agent_irc_nick")) WHERE "agent_irc_nick" is not null;
