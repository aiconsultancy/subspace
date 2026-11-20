//! NETT reads behind the client's operations. Shapes that differ from the
//! desktop's are converted here; the rest go through [`super::wire::normalise`].

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::{query::MAX_PAGE, scope, wire};
use crate::model::{
    DecisionDetail, DecisionListItem, Me, MemberInitiative, Page, Project, WorkflowState,
};
use crate::{Client, DecisionQuery, Error, PageRequest, transport};

/// NETT pages decisions 50 at a time.
const MAX_DECISION_PAGE: u32 = 50;
/// The most organisation members read for the assignee picker.
const MAX_MEMBER_PAGES: u32 = 50;

// qual:allow(coupling, oi) reason: "The NETT read boundary composes organisation routes, the shared transport and the JSON mapping."
impl Client {
    /// An organisation route read as the desktop model `T`.
    pub(crate) fn nett_get<T: DeserializeOwned>(
        &self,
        path: &[&str],
        query: &[(String, String)],
    ) -> Result<T, Error> {
        let query = self.nett_initiative_scope(query)?;
        let mut value = self.nett_value(path, &query)?;
        wire::normalise(&mut value);
        serde_json::from_value(value).map_err(|_| Error::InvalidResponse)
    }

    /// An initiative filter as NETT's repeated planning project filter. An
    /// initiative without projects matches nothing, so it filters by the
    /// nil UUID, which no project has.
    fn nett_initiative_scope(
        &self,
        query: &[(String, String)],
    ) -> Result<Vec<(String, String)>, Error> {
        let mut scoped = Vec::with_capacity(query.len());
        for (key, value) in query {
            if key != "initiative" {
                scoped.push((key.clone(), value.clone()));
                continue;
            }
            let projects = self.nett_initiative_projects(value)?;
            let projects = if projects.is_empty() {
                vec![uuid::Uuid::nil().to_string()]
            } else {
                projects
            };
            scoped.extend(
                projects
                    .into_iter()
                    .map(|id| ("planningProject".to_owned(), id)),
            );
        }
        Ok(scoped)
    }

    /// An initiative's planning projects, read at most once a minute.
    fn nett_initiative_projects(&self, initiative: &str) -> Result<Vec<String>, Error> {
        let key = (
            self.base.to_string(),
            self.account_key(),
            self.org.clone().unwrap_or_default(),
            initiative.to_owned(),
        );
        if let Some(projects) = scope::initiative_projects(&key) {
            return Ok(projects);
        }
        let rows = self.nett_all(&["initiatives", initiative, "projects"], &[])?;
        let projects: Vec<String> = rows
            .iter()
            .filter_map(|row| row.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect();
        scope::remember_initiative_projects(key, projects.clone());
        Ok(projects)
    }

    /// An organisation route's JSON, unmapped.
    pub(crate) fn nett_value(
        &self,
        path: &[&str],
        query: &[(String, String)],
    ) -> Result<Value, Error> {
        let url = self.endpoint(path, query)?;
        self.nett_read(&url)
    }

    /// One organisation read. The same read already in flight for this
    /// account is shared, and a busy answer is tried again after the pause
    /// the flow control applies.
    pub(crate) fn nett_read(&self, url: &url::Url) -> Result<Value, Error> {
        let value = self.coalesced_read(url)?;
        // A body read after the account or organisation changed belongs to
        // no one, however the read was shared.
        self.current()?;
        #[cfg(test)]
        crate::native::test_lease::read_accepted();
        Ok(value)
    }

    /// The issue `reference`, read afresh within `timeout`: never a read
    /// already in flight, which could predate a change just made, and tried
    /// once. A reply read after the account or organisation changed belongs
    /// to no one.
    pub(crate) fn nett_issue_within(
        &self,
        reference: &str,
        timeout: std::time::Duration,
    ) -> Result<crate::model::IssueDetail, Error> {
        let url = self.endpoint(&["issues", reference], &[])?;
        let mut value: Value =
            transport::get_within(&self.agent, &url, self.access_jwt.as_ref(), timeout)?;
        self.current()?;
        #[cfg(test)]
        crate::native::test_lease::read_accepted();
        wire::normalise(&mut value);
        serde_json::from_value(value).map_err(|_| Error::InvalidResponse)
    }

    fn coalesced_read(&self, url: &url::Url) -> Result<Value, Error> {
        super::flow::coalesce(self.account_key(), url.as_str(), || {
            let mut attempt = 1;
            loop {
                match transport::get(&self.agent, url, self.access_jwt.as_ref()) {
                    // Tried again only after a pause short enough to wait
                    // out; a longer one is reported, never cut short.
                    Err(Error::UnexpectedStatus(429 | 503))
                        if attempt < super::flow::READ_ATTEMPTS
                            && super::flow::paused_for(self.flow_account())
                                .is_none_or(|wait| wait <= super::flow::MAX_WAIT) =>
                    {
                        attempt += 1;
                    }
                    other => return other,
                }
            }
        })
    }

    /// `/auth/context`: the caller, and the organisations it can use. The
    /// organisation selection is resolved from it, so a new or saved choice
    /// takes effect, and a choice that changed retires this client.
    pub(crate) fn nett_me(&self) -> Result<Me, Error> {
        // The account the request is made for. A reply whose body arrives
        // after the account changed belongs to no one and is dropped.
        let opened = crate::native::account_generation();
        let (me, organisations) = self.nett_context()?;
        if self.native_lease.is_some() {
            if opened != crate::native::account_generation() {
                return Err(Error::SignInNeeded);
            }
            let saved = scope::saved::load(self.base.as_str(), &me.user_id);
            scope::observe(opened, &me.user_id, organisations, saved);
        }
        Ok(me)
    }

    /// The caller and every organisation it can see, eligible or not.
    pub(crate) fn nett_context(&self) -> Result<(Me, Vec<scope::Organisation>), Error> {
        let url = transport::endpoint(&self.base, &["auth", "context"], &[])?;
        let context: Value = transport::get(&self.agent, &url, self.access_jwt.as_ref())?;
        let me = wire::me(&context).ok_or(Error::SignInNeeded)?;
        Ok((me, wire::organisations(&context)))
    }

    /// The organisation's initiatives, without channels or fleets.
    pub(crate) fn nett_initiatives(&self) -> Result<Vec<MemberInitiative>, Error> {
        let rows = self.nett_all(&["initiatives"], &[])?;
        Ok(rows
            .iter()
            .filter_map(wire::initiative)
            .filter_map(|value| serde_json::from_value(value).ok())
            .collect())
    }

    /// Whether NETT serves `feature` for this client's organisation, reading
    /// its capability list the first time it is needed and again once it is
    /// due: after [`crate::backend::SERVED_FOR`], or after an explicit
    /// reload. A list read before still answers while it cannot be read
    /// again.
    pub(crate) fn nett_serves(&self, feature: crate::backend::Feature) -> Result<bool, Error> {
        let Some(org) = self.org.as_deref() else {
            return Ok(false);
        };
        let origin = self.base.as_str();
        let account = self.flow_account();
        if !crate::backend::served_current(origin, org, account) {
            match self.nett_refresh_served() {
                Ok(_) => {}
                Err(_) if crate::backend::served_known(origin, org, account) => {
                    crate::backend::served_unreadable(origin, org, account);
                }
                // A NETT without the list serves none of the coming features.
                Err(error) => return Err(error),
            }
        }
        Ok(crate::backend::nett_serves(
            origin,
            Some(org),
            account,
            feature,
        ))
    }

    /// Whether the organisation's capability list was read and is not yet
    /// due to be read again.
    pub(crate) fn nett_served_current(&self) -> bool {
        self.org.as_deref().is_some_and(|org| {
            crate::backend::served_current(self.base.as_str(), org, self.flow_account())
        })
    }

    /// Which revision of the organisation's capability list is held, `None`
    /// before one was read: it moves whenever any read finds it changed.
    pub(crate) fn nett_served_revision(&self) -> Option<u64> {
        self.org.as_deref().and_then(|org| {
            crate::backend::served_revision(self.base.as_str(), org, self.flow_account())
        })
    }

    /// Keeps a capability list that could not be read again answering for a
    /// while, so the failing read is not repeated at once.
    pub(crate) fn nett_served_unreadable(&self) {
        if let Some(org) = self.org.as_deref() {
            crate::backend::served_unreadable(self.base.as_str(), org, self.flow_account());
        }
    }

    /// An explicit reload: a pause NETT asked for ends, so what the person
    /// asked for is sent now, and the organisation's capability list is read
    /// again, so a feature NETT enabled meanwhile is offered. Subspace API
    /// has neither.
    pub(crate) fn resume_nett(&self) {
        if self.backend == crate::Backend::Nett {
            crate::nett::flow::resume(self.flow_account());
            self.nett_reread_served();
        }
    }

    /// Asks for the organisation's capability list to be read again the
    /// next time a feature is checked, as an explicit reload does.
    pub(crate) fn nett_reread_served(&self) {
        if let Some(org) = self.org.as_deref() {
            crate::backend::reread_served(self.base.as_str(), org, self.flow_account());
        }
    }

    /// Reads the organisation's capability list again, replacing what was
    /// known: after the update stream closed or was refused, the
    /// organisation may serve less, or more, than before. Returns whether
    /// the list changed.
    ///
    /// The list is kept for the account the read was made for. If another
    /// account signed in while it was read, it belongs to no one and is
    /// dropped, even when both accounts share the organisation.
    pub(crate) fn nett_refresh_served(&self) -> Result<bool, Error> {
        let Some(org) = self.org.as_deref() else {
            return Ok(false);
        };
        let account = self.flow_account();
        let names = match self.nett_value(&["desktop", "capabilities"], &[]) {
            Ok(list) => list
                .get("features")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            Err(Error::NotFound | Error::Forbidden) => Vec::new(),
            Err(error) => return Err(error),
        };
        if self.flow_account() != account {
            return Err(Error::SignInNeeded);
        }
        Ok(crate::backend::record_served(
            self.base.as_str(),
            org,
            account,
            &crate::backend::parse_features(&names),
        ))
    }

    /// Issue teams as the desktop's projects.
    pub(crate) fn nett_teams(&self, page: &PageRequest) -> Result<Page<Project>, Error> {
        // The first organisation read also learns what NETT serves.
        if page.cursor.is_none() {
            let _ = self.nett_serves(crate::backend::Feature::InitiativeChannels);
        }
        let mut parameters = page.parameters(crate::query::MAX_LIST_PAGE_SIZE)?;
        parameters.push(("includeArchived".into(), "true".into()));
        self.nett_get(&["issue-teams"], &parameters)
    }

    /// A team's workflow states come with the team, read by key or ID.
    pub(crate) fn nett_workflow_states(&self, team: &str) -> Result<Page<WorkflowState>, Error> {
        let team: Value = self.nett_get(&["issue-teams", team], &[])?;
        let items = team.get("states").cloned().unwrap_or_else(|| json!([]));
        Ok(Page {
            items: serde_json::from_value(items).map_err(|_| Error::InvalidResponse)?,
            next_cursor: None,
        })
    }

    /// A team's estimate scale.
    pub(crate) fn nett_estimate_scale(&self, team: &str) -> Result<Option<String>, Error> {
        let team = self.nett_value(&["issue-teams", team], &[])?;
        Ok(team
            .get("estimateScale")
            .and_then(Value::as_str)
            .filter(|scale| *scale != "notUsed")
            .map(str::to_owned))
    }

    /// Every page of a cursor-paged organisation collection.
    pub(crate) fn nett_all(
        &self,
        path: &[&str],
        filters: &[(String, String)],
    ) -> Result<Vec<Value>, Error> {
        let mut rows = Vec::new();
        let mut cursor: Option<String> = None;
        let mut seen = std::collections::HashSet::new();
        loop {
            let mut query = filters.to_vec();
            query.push(("limit".into(), MAX_PAGE.to_string()));
            query.extend(
                cursor
                    .iter()
                    .map(|cursor| ("cursor".into(), cursor.clone())),
            );
            let page = self.nett_value(path, &query)?;
            rows.extend(
                page.get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            cursor = page
                .get("nextCursor")
                .and_then(Value::as_str)
                .filter(|cursor| !cursor.is_empty())
                .map(str::to_owned);
            let Some(next) = &cursor else {
                return Ok(rows);
            };
            if !seen.insert(next.clone()) || seen.len() > crate::query::MAX_SCAN_PAGES {
                return Err(Error::InvalidResponse);
            }
        }
    }

    /// Decisions about an issue, newest first. NETT filters by the issue's
    /// UUID and has one status filter, so other statuses are filtered here.
    pub(crate) fn nett_decisions(
        &self,
        query: &DecisionQuery,
        page: &PageRequest,
    ) -> Result<Page<DecisionListItem>, Error> {
        let mut parameters = vec![("status".to_owned(), "all".to_owned())];
        if let Some(issue) = &query.issue {
            parameters.push(("issueId".into(), self.nett_issue_id(issue)?));
        }
        if let Some(search) = query.q.as_ref().filter(|q| !q.trim().is_empty()) {
            parameters.push(("search".into(), search.trim().to_owned()));
        }
        parameters.push((
            "limit".into(),
            page.limit.clamp(1, MAX_DECISION_PAGE).to_string(),
        ));
        parameters.extend(
            page.cursor
                .iter()
                .map(|cursor| ("cursor".into(), cursor.clone())),
        );
        let reply = self.nett_value(&["decisions"], &parameters)?;
        let items = reply
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|item| serde_json::from_value::<DecisionListItem>(wire::decision(item)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Error::InvalidResponse)?;
        let wanted = &query.status;
        Ok(Page {
            items: items
                .into_iter()
                .filter(|item| wanted.is_empty() || wanted.contains(&item.status))
                .collect(),
            next_cursor: reply
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }

    pub(crate) fn nett_decision(&self, id: &str) -> Result<DecisionDetail, Error> {
        let value = self.nett_value(&["decisions", id], &[])?;
        serde_json::from_value(wire::decision(&value)).map_err(|_| Error::InvalidResponse)
    }

    /// An issue's UUID from its identifier; a UUID is returned as it is.
    pub(crate) fn nett_issue_id(&self, reference: &str) -> Result<String, Error> {
        if uuid::Uuid::parse_str(reference).is_ok() {
            return Ok(reference.to_owned());
        }
        let issue = self.nett_value(&["issues", reference], &[])?;
        issue
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(Error::InvalidResponse)
    }

    pub(crate) fn nett_pull_requests(
        &self,
        reference: &str,
        page: &PageRequest,
    ) -> Result<Page<crate::model::PullRequest>, Error> {
        let reply = self.nett_value(
            &["issues", reference, "pull-requests"],
            &page.parameters(crate::query::MAX_RELATION_PAGE_SIZE)?,
        )?;
        let items = reply
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|item| serde_json::from_value(wire::pull_request(item)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Error::InvalidResponse)?;
        Ok(Page {
            items,
            next_cursor: reply
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }

    /// Active members as assignee choices, by user ID.
    pub(crate) fn nett_members(&self) -> Result<Vec<(String, String)>, Error> {
        let mut members = Vec::new();
        for number in 1..=MAX_MEMBER_PAGES {
            let reply = self.nett_value(
                &["members"],
                &[
                    ("page".into(), number.to_string()),
                    ("pageSize".into(), MAX_PAGE.to_string()),
                ],
            )?;
            let items: Vec<Value> = reply
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            members.extend(items.iter().filter_map(wire::member));
            let total = reply.get("totalCount").and_then(Value::as_u64).unwrap_or(0);
            if items.is_empty() || u64::from(number) * u64::from(MAX_PAGE) >= total {
                return Ok(members);
            }
        }
        Ok(members)
    }

    /// Active issue labels.
    pub(crate) fn nett_labels(&self) -> Result<Vec<crate::model::Label>, Error> {
        let reply = self.nett_value(&["labels"], &[("usedFor".into(), "issues".into())])?;
        let items = reply.get("items").cloned().unwrap_or_else(|| json!([]));
        let labels: Vec<crate::model::Label> =
            serde_json::from_value(items).map_err(|_| Error::InvalidResponse)?;
        Ok(labels
            .into_iter()
            .filter(|label| label.state == "active")
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::{Feature, SERVED_FOR, served_known};
    use crate::native::test_lease;
    use crate::{Client, Error};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use subspace_framework::client_test_support::Mock;
    use subspace_framework::core::Secret;

    const ORG: &str = "00000000-0000-4000-8000-0000000000c1";

    fn client(mock: &Mock, token: &str) -> Client {
        Client::nett_loopback(
            &format!("{}/api", mock.base_url()),
            Some(ORG),
            Some(Secret::new(token)),
        )
        .unwrap()
        .without_live()
    }

    /// Serves the capability list, attachments and updates once `enabled`
    /// and a 404 before, or a 500 once enabled when `broken`; counts the
    /// reads.
    fn capabilities(enabled: &Arc<AtomicBool>, reads: &Arc<AtomicUsize>, broken: bool) -> Mock {
        let (enabled, reads) = (Arc::clone(enabled), Arc::clone(reads));
        Mock::new(move |request| {
            if !request.path.ends_with("/desktop/capabilities") {
                return (404, "{}".into());
            }
            reads.fetch_add(1, Ordering::SeqCst);
            match (
                broken && enabled.load(Ordering::SeqCst),
                enabled.load(Ordering::SeqCst),
            ) {
                (true, _) => (500, "{}".into()),
                (false, true) => (200, r#"{"features":["attachments","updates"]}"#.into()),
                (false, false) => (404, "{}".into()),
            }
        })
    }

    #[test]
    fn a_capability_list_read_as_another_account_signs_in_is_kept_for_no_one() {
        let mock = Mock::new(|request| {
            if request.path.ends_with("/desktop/capabilities") {
                (200, r#"{"features":["attachments","updates"]}"#.into())
            } else {
                (404, "{}".into())
            }
        });
        let mut client = client(&mock, "nett-capabilities-switch");
        // Bound to account 1's lease, as a native client is.
        test_lease::set(Some(1));
        test_lease::set_account(Some(1));
        client.native_lease = Some(crate::NativeLease(Some(1)));
        // Account 2, in the same organisation, signs in right after the
        // list's reply was accepted, before it is kept.
        test_lease::AFTER_READ.with(|next| next.set(Some(2)));
        let result = client.nett_refresh_served();
        test_lease::set(None);
        test_lease::set_account(None);
        let origin = client.base.as_str();
        assert_eq!(result, Err(Error::SignInNeeded));
        assert!(
            !served_known(origin, ORG, 2),
            "account 2 never inherits account 1's features"
        );
        assert!(
            !served_known(origin, ORG, 1),
            "and the stale read is dropped"
        );
    }

    #[test]
    fn a_capability_list_read_before_a_feature_was_enabled_is_read_again() {
        let (enabled, reads) = (Arc::new(AtomicBool::new(false)), Arc::default());
        let mock = capabilities(&enabled, &reads, false);
        let client = client(&mock, "nett-capabilities-enabled");
        assert_eq!(client.nett_serves(Feature::Attachments), Ok(false));
        assert_eq!(client.nett_serves(Feature::Updates), Ok(false));
        assert_eq!(reads.load(Ordering::SeqCst), 1, "the absent list is kept");
        // NETT enables the features while the app runs; an explicit reload
        // reads the list again.
        enabled.store(true, Ordering::SeqCst);
        client.nett_reread_served();
        assert!(!client.nett_served_current());
        assert_eq!(client.nett_serves(Feature::Attachments), Ok(true));
        assert_eq!(client.nett_serves(Feature::Updates), Ok(true));
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        // Without a reload, a list past its lifetime is read again too.
        let later = std::time::Instant::now() + SERVED_FOR + std::time::Duration::from_secs(1);
        assert!(crate::backend::served_current(
            client.base.as_str(),
            ORG,
            client.flow_account()
        ));
        assert!(!crate::backend::served_current_at(
            client.base.as_str(),
            ORG,
            client.flow_account(),
            later
        ));
    }

    #[test]
    fn a_capability_list_that_cannot_be_read_again_still_answers() {
        let (enabled, reads) = (Arc::new(AtomicBool::new(false)), Arc::default());
        let mock = capabilities(&enabled, &reads, true);
        let client = client(&mock, "nett-capabilities-broken");
        assert_eq!(client.nett_serves(Feature::Attachments), Ok(false));
        // The list is due again, but NETT fails to answer: what was known
        // still answers, and the failing read is not repeated at once.
        enabled.store(true, Ordering::SeqCst);
        client.nett_reread_served();
        assert_eq!(client.nett_serves(Feature::Attachments), Ok(false));
        assert_eq!(client.nett_serves(Feature::Updates), Ok(false));
        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }
}
