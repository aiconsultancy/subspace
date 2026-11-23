ALTER TABLE "users" ADD COLUMN "token_role" text;--> statement-breakpoint
ALTER TABLE "users" ADD CONSTRAINT "users_token_role" CHECK ("token_role" is null or ("token_role" in ('admin', 'member') and "agent_rank" is null));
