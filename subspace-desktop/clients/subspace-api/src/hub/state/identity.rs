//! A verified account transition retires jobs and cached account data.

use super::State;
use crate::Error;

impl State {
    pub(in crate::hub) fn retire_if_account_changed(
        &mut self,
        identity: &Result<String, Error>,
        boot_error: Option<Error>,
    ) -> bool {
        let refused = matches!(boot_error, Some(Error::SignInNeeded | Error::Forbidden));
        let changed = self.snapshot.me.as_ref().is_some_and(|previous| {
            identity
                .as_ref()
                .is_ok_and(|actor| actor != &previous.user_id)
        });
        if refused || changed {
            self.retire_account();
        }
        refused || changed
    }

    fn retire_account(&mut self) {
        self.snapshot.generation.0 = self.snapshot.generation.0.wrapping_add(1);
        self.snapshot.me = None;
        self.snapshot.projects.clear();
        self.snapshot.issue = None;
        self.snapshot.live = crate::hub::LiveStatus::Connecting;
        self.clear_list();
        let activity = self.pending_activity.take().or_else(|| {
            self.snapshot
                .activity
                .as_ref()
                .map(|view| view.query().clone())
        });
        self.clear_activity();
        self.pending_activity = activity;
        self.project_refresh = Default::default();
        self.live_reads = Default::default();
        self.reset_references();
        self.drafts.clear();
        self.parked_uploads.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::{
        Error, ListQuery, ListTab,
        hub::worker::{Job, Task},
    };

    #[test]
    fn account_refusal_retires_an_in_flight_list_after_a_project_refresh() {
        let mut state = State::new("https://fixture.example", "Fixture");
        let list = Job::new(&state, Task::List(ListQuery::new(ListTab::All)));
        assert!(list.current(&state));
        assert!(!state.retire_if_account_changed(&Ok("fixture".into()), None));
        assert!(list.current(&state));

        assert!(state.retire_if_account_changed(&Err(Error::Forbidden), Some(Error::Forbidden)));
        assert!(!list.current(&state));
    }
    #[test]
    fn a_new_account_cannot_reuse_the_previous_accounts_fresh_list() {
        use crate::model::{IssueGroups, Me};

        let mut state = State::new("https://fixture.example", "Fixture");
        state.snapshot.me = Some(Me {
            user_id: "first".into(),
            kind: "human".into(),
            display_name: "Fixture".into(),
        });
        state.install_list(IssueGroups {
            group_by: "stateType".into(),
            groups: Vec::new(),
        });

        assert!(state.retire_if_account_changed(&Ok("second".into()), None));
        assert!(state.snapshot.list().is_none());
        assert!(state.snapshot.updated_at().is_none());
        assert!(state.snapshot.me().is_none());
    }
    #[test]
    fn an_account_switch_discards_activity_rows_and_retains_the_read_intent() {
        use crate::{ActivityQuery, hub::ActivitySnapshot, model::Me};

        let mut state = State::new("https://fixture.example", "Fixture");
        state.snapshot.me = Some(Me {
            user_id: "first".into(),
            kind: "human".into(),
            display_name: "Fixture".into(),
        });
        let query = ActivityQuery::default().project(Some("SUB"));
        state.snapshot.activity = Some(ActivitySnapshot::new(query.clone()));

        assert!(state.retire_if_account_changed(&Ok("second".into()), None));
        assert!(state.snapshot.activity().is_none());
        assert_eq!(state.pending_activity, Some(query));
    }
}
