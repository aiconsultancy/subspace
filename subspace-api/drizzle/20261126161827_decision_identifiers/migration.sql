ALTER TABLE "project_decisions" ADD COLUMN "identifier" text;
--> statement-breakpoint
ALTER TABLE "projects" ADD COLUMN "next_decision_number" integer DEFAULT 1 NOT NULL;
--> statement-breakpoint
WITH numbered AS (
  SELECT d.id, p.key, row_number() OVER (
    PARTITION BY d.organisation_id, d.project_id ORDER BY d.created_at, d.id
  ) AS number
  FROM project_decisions d
  JOIN projects p ON p.organisation_id = d.organisation_id AND p.id = d.project_id
)
UPDATE project_decisions d
SET identifier = 'D-' || lpad(n.number::text, greatest(5, length(n.number::text)), '0') || '-' || n.key
FROM numbered n
WHERE n.id = d.id;
--> statement-breakpoint
UPDATE projects p
SET next_decision_number = counts.total + 1
FROM (
  SELECT organisation_id, project_id, count(*) AS total
  FROM project_decisions
  GROUP BY organisation_id, project_id
) counts
WHERE p.organisation_id = counts.organisation_id AND p.id = counts.project_id;
--> statement-breakpoint
ALTER TABLE "project_decisions" ALTER COLUMN "identifier" SET NOT NULL;
--> statement-breakpoint
ALTER TABLE "project_decisions" ADD CONSTRAINT "project_decisions_organisation_id_identifier_unique" UNIQUE("organisation_id","identifier");
--> statement-breakpoint
-- Add identifiers to saved snapshots without changing their historical content or metadata.
WITH identifiers AS (
  SELECT r.id, d.identifier, predecessor.identifier AS predecessor, successor.identifier AS successor
  FROM project_decision_revisions r
  JOIN project_decisions d ON d.organisation_id = r.organisation_id AND d.id = r.decision_id
  LEFT JOIN project_decisions predecessor ON predecessor.organisation_id = r.organisation_id
    AND predecessor.id = (r.snapshot -> 'supersedes' ->> 'id')::uuid
  LEFT JOIN project_decisions successor ON successor.organisation_id = r.organisation_id
    AND successor.id = (r.snapshot -> 'supersededBy' ->> 'id')::uuid
)
UPDATE project_decision_revisions r
SET snapshot = r.snapshot || jsonb_build_object('identifier', i.identifier)
  || CASE WHEN i.predecessor IS NULL THEN '{}'::jsonb ELSE
    jsonb_build_object('supersedes', (r.snapshot -> 'supersedes') || jsonb_build_object('identifier', i.predecessor))
  END
  || CASE WHEN i.successor IS NULL THEN '{}'::jsonb ELSE
    jsonb_build_object('supersededBy', (r.snapshot -> 'supersededBy') || jsonb_build_object('identifier', i.successor))
  END
FROM identifiers i
WHERE i.id = r.id;
