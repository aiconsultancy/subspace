-- Additive and safe to run twice: existing issues start as not ready for agents.
ALTER TABLE "issues" ADD COLUMN IF NOT EXISTS "ready_for_agents" boolean DEFAULT false NOT NULL;
