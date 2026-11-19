CREATE TABLE "initiative_stakeholders" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid(),
	"organisation_id" uuid NOT NULL,
	"initiative_id" uuid NOT NULL,
	"name" text NOT NULL,
	"role" text NOT NULL,
	"slack_handle" text,
	"teams_handle" text,
	CONSTRAINT "stakeholder_contact" CHECK ("slack_handle" is not null or "teams_handle" is not null)
);
--> statement-breakpoint
CREATE INDEX "initiative_stakeholders_organisation_id_initiative_id_index" ON "initiative_stakeholders" ("organisation_id","initiative_id");--> statement-breakpoint
ALTER TABLE "initiative_stakeholders" ADD CONSTRAINT "initiative_stakeholders_organisation_id_organisations_id_fkey" FOREIGN KEY ("organisation_id") REFERENCES "organisations"("id") ON DELETE CASCADE;--> statement-breakpoint
ALTER TABLE "initiative_stakeholders" ADD CONSTRAINT "initiative_stakeholders_pOM8ZHgItmj1_fkey" FOREIGN KEY ("organisation_id","initiative_id") REFERENCES "initiatives"("organisation_id","id") ON DELETE CASCADE;
