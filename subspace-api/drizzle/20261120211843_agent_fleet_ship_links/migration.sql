CREATE TABLE "ships" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"fleet_id" uuid NOT NULL,
	"name" text NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "ships_scope_id" UNIQUE("organisation_id","fleet_id","id")
);
--> statement-breakpoint
ALTER TABLE "fleets" DROP CONSTRAINT "fleets_captain_names";--> statement-breakpoint
ALTER TABLE "project_decisions" ADD COLUMN "made_by_type" text;--> statement-breakpoint
ALTER TABLE "project_decisions" ADD COLUMN "made_by_user_id" uuid;--> statement-breakpoint
ALTER TABLE "project_decisions" ADD COLUMN "made_by_agent_name" text;--> statement-breakpoint
ALTER TABLE "project_decisions" ADD COLUMN "made_by_agent_rank" text;--> statement-breakpoint
ALTER TABLE "project_decisions" ADD COLUMN "made_by_agent_model" text;--> statement-breakpoint
ALTER TABLE "fleets" ADD COLUMN "version" integer DEFAULT 1 NOT NULL;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "agent_fleet_id" uuid;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "agent_ship_id" uuid;--> statement-breakpoint
ALTER TABLE "fleets" ALTER COLUMN "irc_host" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "fleets" ALTER COLUMN "irc_port" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "fleets" ALTER COLUMN "irc_tls" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "fleets" ALTER COLUMN "rear_admiral_name" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "fleets" ALTER COLUMN "captain_names" DROP NOT NULL;--> statement-breakpoint
CREATE UNIQUE INDEX "ships_fleet_name" ON "ships" ("fleet_id",lower("name"));--> statement-breakpoint
CREATE INDEX "users_agent_fleet" ON "users" ("agent_fleet_id");--> statement-breakpoint
ALTER TABLE "project_decisions" ADD CONSTRAINT "project_decisions_IYiWUiqhpLXs_fkey" FOREIGN KEY ("organisation_id","made_by_user_id") REFERENCES "users"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "ships" ADD CONSTRAINT "ships_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "ships" ADD CONSTRAINT "ships_organisation_id_fleet_id_fleets_organisation_id_id_fkey" FOREIGN KEY ("organisation_id","fleet_id") REFERENCES "fleets"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "users" ADD CONSTRAINT "users_VZzs4FXQtVZz_fkey" FOREIGN KEY ("organisation_id","agent_fleet_id") REFERENCES "fleets"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "users" ADD CONSTRAINT "users_njO6vz804QAF_fkey" FOREIGN KEY ("organisation_id","agent_fleet_id","agent_ship_id") REFERENCES "ships"("organisation_id","fleet_id","id");--> statement-breakpoint
ALTER TABLE "users" ADD CONSTRAINT "agent_ship_requires_fleet" CHECK ("agent_ship_id" is null or "agent_fleet_id" is not null);

--> statement-breakpoint
UPDATE users AS agent
SET agent_fleet_id = fleet.id
FROM fleets AS fleet
WHERE agent.agent_rank IS NOT NULL
  AND agent.agent_fleet_id IS NULL
  AND fleet.organisation_id = agent.organisation_id
  AND fleet.slug = lower(regexp_replace(agent.agent_fleet, '^[[:space:]]+|[[:space:]]+$', '', 'g'));

--> statement-breakpoint
INSERT INTO ships (organisation_id, fleet_id, name)
SELECT DISTINCT ON (fleet_id, lower(name)) organisation_id, fleet_id, name
FROM (
    SELECT organisation_id, agent_fleet_id AS fleet_id,
           regexp_replace(agent_ship, '^[[:space:]]+|[[:space:]]+$', '', 'g') AS name
    FROM users
    WHERE agent_fleet_id IS NOT NULL AND agent_ship_id IS NULL AND agent_rank <> 'rearAdmiral'
) legacy
WHERE name <> ''
ORDER BY fleet_id, lower(name), name COLLATE "C"
ON CONFLICT (fleet_id, lower(name)) DO NOTHING;

--> statement-breakpoint
UPDATE users AS agent
SET agent_ship_id = ship.id
FROM ships AS ship
WHERE agent.agent_fleet_id = ship.fleet_id
  AND agent.organisation_id = ship.organisation_id
  AND agent.agent_ship_id IS NULL
  AND agent.agent_rank <> 'rearAdmiral'
  AND lower(ship.name) = lower(regexp_replace(agent.agent_ship, '^[[:space:]]+|[[:space:]]+$', '', 'g'));

--> statement-breakpoint
-- The previous write path only allowed the recorder to act as the maker.
UPDATE project_decisions AS decision
SET made_by_type = CASE WHEN actor.agent_rank IS NULL THEN 'human' ELSE 'agent' END,
    made_by_user_id = actor.id,
    made_by_agent_name = CASE WHEN actor.agent_rank IS NULL THEN NULL ELSE actor.display_name END,
    made_by_agent_rank = actor.agent_rank,
    made_by_agent_model = actor.agent_model
FROM users AS actor
WHERE decision.organisation_id = actor.organisation_id AND decision.recorded_by_id = actor.id
  AND decision.made_by_type IS NULL;
--> statement-breakpoint
ALTER TABLE project_decisions ALTER COLUMN made_by_type SET NOT NULL;
