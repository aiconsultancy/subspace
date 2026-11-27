CREATE TABLE "decision_outcomes" (
	"organisation_id" uuid,
	"decision_id" uuid,
	"fleet_id" uuid NOT NULL,
	"recorded_by_id" uuid NOT NULL,
	"recorded_by_rank" text NOT NULL,
	"status" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"delivered_at" timestamp with time zone,
	CONSTRAINT "decision_outcomes_pkey" PRIMARY KEY("organisation_id","decision_id"),
	CONSTRAINT "decision_outcomes_status" CHECK ("status" IN ('accepted', 'rejected')),
	CONSTRAINT "decision_outcomes_recorder_rank" CHECK ("recorded_by_rank" IN ('captain', 'rearAdmiral'))
);
--> statement-breakpoint
CREATE INDEX "decision_outcomes_pending" ON "decision_outcomes" ("organisation_id","fleet_id","created_at","decision_id") WHERE "delivered_at" IS NULL;--> statement-breakpoint
ALTER TABLE "decision_outcomes" ADD CONSTRAINT "decision_outcomes_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "decision_outcomes" ADD CONSTRAINT "decision_outcomes_4dtbEVZysXJh_fkey" FOREIGN KEY ("organisation_id","decision_id") REFERENCES "project_decisions"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "decision_outcomes" ADD CONSTRAINT "decision_outcomes_gRIuOkUyN21T_fkey" FOREIGN KEY ("organisation_id","fleet_id") REFERENCES "fleets"("organisation_id","id");--> statement-breakpoint
ALTER TABLE "decision_outcomes" ADD CONSTRAINT "decision_outcomes_w7zveBrYeZYy_fkey" FOREIGN KEY ("organisation_id","recorded_by_id") REFERENCES "users"("organisation_id","id");
--> statement-breakpoint
CREATE FUNCTION record_decision_outcome() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO decision_outcomes (organisation_id, decision_id, fleet_id, recorded_by_id, recorded_by_rank, status)
  SELECT NEW.organisation_id, NEW.id, u.agent_fleet_id, u.id, u.agent_rank, NEW.status
  FROM users u
  WHERE u.organisation_id = NEW.organisation_id AND u.id = NEW.recorded_by_id
    AND u.active AND u.agent_rank IN ('captain', 'rearAdmiral') AND u.agent_fleet_id IS NOT NULL;
  RETURN NEW;
END;
$$;
--> statement-breakpoint
CREATE TRIGGER record_decision_outcome
AFTER UPDATE OF status ON project_decisions
FOR EACH ROW
WHEN (OLD.status = 'proposed' AND NEW.status IN ('accepted', 'rejected'))
EXECUTE FUNCTION record_decision_outcome();
