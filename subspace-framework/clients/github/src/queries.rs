//! The GitHub GraphQL documents.

const ACTOR: &str = "login ... on User { name }";

fn common_fields() -> String {
    format!(
        "id number title url state createdAt updatedAt
  repository {{ nameWithOwner }}
  author {{ {ACTOR} }}
  assignees(first: 10) {{ nodes {{ login name }} }}
  labels(first: 10) {{ nodes {{ name color }} }}
  comments {{ totalCount }}"
    )
}

fn search_query(pr_fields: &str, issue_fields: &str) -> String {
    format!(
        "query Section($query: String!) {{
  search(type: ISSUE, query: $query, first: 50) {{
    issueCount
    nodes {{ __typename ... on PullRequest {{ id {pr_fields} }} ... on Issue {{ id {issue_fields} }} }}
  }}
}}"
    )
}

/// The section rows. PR review decisions and diff stats come from
/// [`pr_extra_queries`]: with everything in one document, 50 PRs can pass
/// GitHub's 10-second query limit (HTTP 502).
pub fn section_query() -> String {
    let common = common_fields();
    search_query(
        &format!(
            "{common} isDraft headRefName baseRefName commits(last: 1) {{ nodes {{ commit {{ statusCheckRollup {{ state }} }} }} }}"
        ),
        &common,
    )
}

pub fn pr_extra_queries() -> [String; 2] {
    [
        search_query("reviewDecision", ""),
        search_query("additions deletions", ""),
    ]
}

fn event_fields() -> String {
    format!(
        "commentEvents: comments(last: 50) {{ nodes {{ author {{ {ACTOR} }} createdAt bodyHTML url }} }}"
    )
}

pub fn detail_query() -> String {
    let common = common_fields();
    let events = event_fields();
    format!(
        "query Item($owner: String!, $name: String!, $number: Int!) {{
  repository(owner: $owner, name: $name) {{
    issueOrPullRequest(number: $number) {{
      __typename
      ... on PullRequest {{ {common}
        isDraft additions deletions headRefName baseRefName reviewDecision
        bodyHTML viewerDidAuthor viewerCanReact locked changedFiles
        latestReviews(first: 100) {{ nodes {{ author {{ {ACTOR} }} state }} }}
        latestOpinionatedReviews(first: 100) {{ nodes {{ author {{ {ACTOR} }} state }} }}
        reviewRequests(first: 100) {{ nodes {{ requestedReviewer {{
          __typename ... on User {{ login name }} ... on Bot {{ login }} ... on Mannequin {{ login }}
          ... on Team {{ combinedSlug name }} }} }} }}
        commits(last: 1) {{ nodes {{ commit {{ statusCheckRollup {{ state
          contexts(first: 100) {{ nodes {{
            __typename
            ... on CheckRun {{ name status conclusion detailsUrl }}
            ... on StatusContext {{ context state targetUrl }} }} }} }} }} }} }}
        {events}
        reviews(last: 50) {{ nodes {{ author {{ {ACTOR} }} createdAt submittedAt bodyHTML body state url }} }}
      }}
      ... on Issue {{ {common} bodyHTML viewerDidAuthor viewerCanReact locked {events} }}
    }}
  }}
}}"
    )
}

pub const ASSIGNABLE_QUERY: &str = "
query Assignable($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) { assignableUsers(first: 100) { nodes { login name } } }
}";

pub const SUBJECT_QUERY: &str = "
query Subject($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    issueOrPullRequest(number: $number) { __typename ... on PullRequest { id } ... on Issue { id } }
  }
}";

pub const REVIEW_MUTATION: &str = "
mutation Review($id: ID!, $event: PullRequestReviewEvent!, $body: String) {
  addPullRequestReview(input: { pullRequestId: $id, event: $event, body: $body }) { pullRequestReview { id } }
}";

pub const COMMENT_MUTATION: &str = "
mutation Comment($id: ID!, $body: String!) {
  addComment(input: { subjectId: $id, body: $body }) { clientMutationId }
}";

pub const VIEWER_QUERY: &str = "query Viewer { viewer { login } }";

/// One query resolving the item id and every login's user id; aliases are
/// indices, logins are variables.
pub fn assignee_lookup_query(count: usize) -> String {
    let params: String = (0..count)
        .map(|index| format!(", $l{index}: String!"))
        .collect();
    let users: Vec<String> = (0..count)
        .map(|index| format!("u{index}: user(login: $l{index}) {{ id login }}"))
        .collect();
    format!(
        "query Assignees($owner: String!, $name: String!, $number: Int!{params}) {{
  repository(owner: $owner, name: $name) {{
    issueOrPullRequest(number: $number) {{ __typename ... on PullRequest {{ id }} ... on Issue {{ id }} }}
  }}
  {}
}}",
        users.join("\n  ")
    )
}

pub fn assignee_mutation(add: bool, remove: bool) -> String {
    let params: Vec<&str> = [(add, "$add: [ID!]!"), (remove, "$remove: [ID!]!")]
        .into_iter()
        .filter_map(|(on, param)| on.then_some(param))
        .collect();
    let add = if add {
        "addAssigneesToAssignable(input: { assignableId: $id, assigneeIds: $add }) { clientMutationId }"
    } else {
        ""
    };
    let remove = if remove {
        "removeAssigneesFromAssignable(input: { assignableId: $id, assigneeIds: $remove }) { clientMutationId }"
    } else {
        ""
    };
    format!(
        "mutation Assign($id: ID!, {}) {{\n  {add}\n  {remove}\n}}",
        params.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::{assignee_lookup_query, assignee_mutation, detail_query, section_query};

    #[test]
    fn documents_have_balanced_braces() {
        for document in [
            section_query(),
            detail_query(),
            assignee_lookup_query(2),
            assignee_mutation(true, true),
        ] {
            let open = document.matches('{').count();
            let close = document.matches('}').count();
            assert_eq!(open, close, "{document}");
        }
    }

    #[test]
    fn the_assignee_lookup_uses_variables_for_logins() {
        let query = assignee_lookup_query(2);
        assert!(query.contains("$l0: String!, $l1: String!"));
        assert!(query.contains("u1: user(login: $l1) { id login }"));
        let mutation = assignee_mutation(false, true);
        assert!(mutation.contains("($id: ID!, $remove: [ID!]!)"));
        assert!(!mutation.contains("addAssignees"));
    }
}
