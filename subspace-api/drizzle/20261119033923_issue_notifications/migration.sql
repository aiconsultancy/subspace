CREATE FUNCTION subspace_notify_issue_activity() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM pg_notify(
    'subspace_issues',
    json_build_object(
      'organisationId', NEW.organisation_id,
      'projectId', i.project_id,
      'projectKey', p.key,
      'issueId', i.id,
      'identifier', i.identifier,
      'parentIdentifier', parent.identifier,
      'version', i.version,
      'action', NEW.kind,
      'actorType', 'human',
      'occurredAt', NEW.created_at
    )::text
  )
  FROM issues i
  JOIN projects p ON p.id = i.project_id AND p.organisation_id = i.organisation_id
  LEFT JOIN issues parent ON parent.id = i.parent_id AND parent.organisation_id = i.organisation_id
  WHERE i.id = NEW.issue_id AND i.organisation_id = NEW.organisation_id;
  RETURN NEW;
END;
$$;
--> statement-breakpoint
CREATE TRIGGER subspace_issue_activity_notification
AFTER INSERT ON "issue_activity"
FOR EACH ROW EXECUTE FUNCTION subspace_notify_issue_activity();
