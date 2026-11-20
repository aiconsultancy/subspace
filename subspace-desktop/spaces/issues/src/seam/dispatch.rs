//! Requests from the board, sent to the hub as calls.

use gpui_kit::Context;
use subspace_api_client::hub::{Hub, HubError};
use subspace_framework::core::log::Logger;

use crate::Issues;
use crate::requests::Request;

// qual:allow(coupling, oi) reason: "The board's requests to the hub are part of its link to the hub in seam.rs, kept apart from the board's own choices."
impl Issues {
    /// Asks for `request`: a hub call at once when there is a hub.
    pub(crate) fn request(&mut self, request: Request, cx: &mut Context<Self>) {
        self.requests.push(request);
        let Some(hub) = self.hub.clone() else {
            return;
        };
        for request in self.take_requests() {
            if let Err(error) = self.send(&hub, request) {
                Logger::new(crate::ID).debug(format!("The Issues hub refused a read: {error}"));
            }
        }
        if !self.syncing {
            // A read the hub didn't need, such as a list under a minute old,
            // leaves its version as it was; its snapshot still settles the
            // board's own *loading*.
            self.seen = None;
        }
        self.pull_snapshot(cx);
    }

    fn send(&self, hub: &Hub, request: Request) -> Result<(), HubError> {
        match request {
            Request::Access => hub.reload(self.choice().query()),
            Request::List(choice) => hub.activate(choice.query()),
            Request::Reload(choice) => hub.reload(choice.query()),
            Request::ReloadProjects => hub.reload_projects(),
            Request::Activity {
                query,
                reload: true,
            } => hub.reload_activity(query),
            Request::Activity {
                query,
                reload: false,
            } => hub.activate_activity(query),
            Request::ActivityPage {
                section,
                more: true,
            } => hub.load_more_activity(section),
            Request::ActivityPage {
                section,
                more: false,
            } => hub.retry_activity(section),
            Request::OpenIssue { identifier } => hub.open_issue(&identifier, &self.profile),
            Request::ReloadIssue => hub.reload_issue(),
            Request::MoreSubissues => hub.load_more_subissues(),
            Request::MoreRelations => hub.load_more_relations(),
            Request::MorePullRequests => hub.load_more_pull_requests(),
            Request::SetTodo { item, done } => hub.set_todo_done(&item, done),
            Request::RetryTodos => hub.reload_todos(),
            Request::OpenTimeline(goal) => hub.activate_timeline(goal),
            Request::MoreTimeline(goal) => hub.load_more_timeline(goal),
            Request::RetryTimeline(goal) => hub.retry_timeline(goal),
            Request::OpenDecisions => hub.activate_decisions(),
            Request::MoreDecisions => hub.load_more_decisions(),
            Request::RetryDecisions => hub.retry_decisions(),
            Request::OpenDecision(id) => hub.open_decision(&id),
            Request::RetryDecision(id) => hub.retry_decision(&id),
            Request::CloseDecision => {
                hub.close_decision();
                Ok(())
            }
            Request::More { group } | Request::Group { group } => match self.group_index(&group) {
                Some(index) => hub.load_more(index),
                None => Ok(()),
            },
        }
    }

    fn group_index(&self, key: &str) -> Option<usize> {
        let groups = &self.listing.content.as_ref()?.groups;
        groups
            .iter()
            .find(|group| group.key == key)
            .map(|group| group.index)
    }
}
