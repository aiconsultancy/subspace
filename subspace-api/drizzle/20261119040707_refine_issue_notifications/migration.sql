CREATE OR REPLACE FUNCTION subspace_notify_issue_activity() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  hint jsonb;
  source_project record;
BEGIN
  SELECT jsonb_build_object(
      'organisationId', NEW.organisation_id,
      'projectId', i.project_id,
      'projectKey', p.key,
      'issueId', i.id,
      'identifier', i.identifier,
      'parentIdentifier', parent.identifier,
      'version', i.version,
      'auditId', NULL,
      'action', CASE NEW.kind
        WHEN 'issue.created' THEN 'issueCreated'
        WHEN 'issue.changed' THEN 'issueUpdated'
        WHEN 'issue.state-derived' THEN 'issueUpdated'
        WHEN 'issue.archived' THEN 'issueArchived'
        WHEN 'issue.unarchived' THEN 'issueUnarchived'
        WHEN 'issue.moved' THEN 'issueMoved'
        WHEN 'comment.created' THEN 'commentCreated'
        WHEN 'decision.logged' THEN 'decisionLogged'
        WHEN 'item.created' THEN 'issueTodoCreated'
        WHEN 'item.removed' THEN 'issueTodoDeleted'
        WHEN 'item.moved' THEN 'issueTodosReordered'
        ELSE 'issueTodoUpdated'
      END,
      'actorType', 'human',
      'occurredAt', NEW.created_at
    )
    INTO hint
  FROM issues i
  JOIN projects p ON p.id = i.project_id AND p.organisation_id = i.organisation_id
  LEFT JOIN issues parent ON parent.id = i.parent_id AND parent.organisation_id = i.organisation_id
  WHERE i.id = NEW.issue_id AND i.organisation_id = NEW.organisation_id;
  IF hint IS NOT NULL THEN
    PERFORM pg_notify('subspace_issues', hint::text);
  END IF;
  IF NEW.kind = 'issue.moved' AND NEW.data ? 'sourceProjectId' THEN
    SELECT p.id, p.key INTO source_project FROM projects p
    WHERE p.id = (NEW.data->>'sourceProjectId')::uuid AND p.organisation_id = NEW.organisation_id;
    IF FOUND THEN
      PERFORM pg_notify('subspace_issues', (hint || jsonb_build_object(
        'projectId', source_project.id, 'projectKey', source_project.key
      ))::text);
    END IF;
  END IF;
  RETURN NEW;
END;
$$;
