CREATE OR REPLACE FUNCTION subspace_audit_issue_activity() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO audit_logs (id, organisation_id, occurred_at, action, "actorType", actor_id,
    actor_display_name, actor_rank, source, entity_type, entity_id, project_id, summary, changes)
  SELECT NEW.id, NEW.organisation_id, NEW.created_at,
    CASE NEW.kind
      WHEN 'issue.created' THEN 'issueCreated'
      WHEN 'issue.changed' THEN CASE WHEN NEW.data ? 'previousStateId' OR NEW.source = 'github'
        THEN 'issueStatusChanged' ELSE 'issueUpdated' END
      WHEN 'issue.state-derived' THEN 'issueStatusChanged'
      WHEN 'issue.archived' THEN 'issueArchived'
      WHEN 'issue.unarchived' THEN 'issueUnarchived'
      WHEN 'issue.moved' THEN 'issueMoved'
      WHEN 'comment.created' THEN 'issueCommentAdded'
      WHEN 'comment.edited' THEN 'issueCommentEdited'
      WHEN 'comment.deleted' THEN 'issueCommentDeleted'
      WHEN 'comment.resolved' THEN 'issueCommentResolutionChanged'
      WHEN 'decision.logged' THEN 'decisionLogged'
      WHEN 'item.created' THEN 'issueTodoAdded'
      WHEN 'item.removed' THEN 'issueTodoDeleted'
      WHEN 'item.edited' THEN 'issueTodoUpdated'
      WHEN 'item.completed' THEN 'issueTodoCompleted'
      WHEN 'item.reopened' THEN 'issueTodoReopened'
      WHEN 'item.moved' THEN 'issueTodosReordered'
      ELSE NEW.kind
    END,
    CASE WHEN NEW.actor_id IS NULL THEN 'integration'
      WHEN u.agent_rank IS NOT NULL THEN 'agent' ELSE 'human' END,
    NEW.actor_id, COALESCE(u.display_name, 'GitHub'), u.agent_rank, NEW.source,
    'Issue', NEW.issue_id::text, i.project_id, NEW.kind, NEW.data
  FROM issues i
  LEFT JOIN users u ON u.id = NEW.actor_id AND u.organisation_id = NEW.organisation_id
  WHERE i.id = NEW.issue_id AND i.organisation_id = NEW.organisation_id;
  RETURN NEW;
END;
$$;
