//! What a reconnect keeps of a failed chat. The new runtime restores the
//! saved chats with new tabs, so each kept failure goes to the restored tab
//! of the same SDK session. A chat whose first turn failed was never saved,
//! so its session does not come back: when exactly one such chat failed, its
//! failure goes to the new runtime's first empty chat, so Retry can send
//! the message again. Other unmatched failures are dropped rather than shown
//! on another chat.

use subspace_agent::TurnFailure;

use super::tab_run::ShownFailure;
use super::tabs::{LastTurn, Tab, session_tab};

/// A failed chat's failure and the message Retry sends, by session.
#[derive(Clone, Debug)]
pub(super) struct Carried {
    session: Option<String>,
    failure: TurnFailure,
    last_turn: Option<LastTurn>,
}

/// Takes the failure and the last message of every tab that has a failure.
pub(super) fn take(tabs: &mut [Tab]) -> Vec<Carried> {
    tabs.iter_mut()
        .filter_map(|tab| {
            let shown = tab.failure.take()?;
            Some(Carried {
                session: tab.session.clone(),
                failure: shown.failure,
                last_turn: tab.last_turn.take(),
            })
        })
        .collect()
}

/// Gives each kept failure to the restored tab of its session, and a single
/// unsaved one to the first empty chat.
pub(super) fn restore(tabs: &mut [Tab], carried: Vec<Carried>) {
    let unsaved: Vec<Carried> = carried
        .into_iter()
        .filter_map(|kept| place_by_session(tabs, kept))
        .collect();
    if let [kept] = unsaved.as_slice() {
        place_in_empty_chat(tabs, kept.clone());
    }
}

/// Puts `kept` on the tab of its session; returns it when none matches.
fn place_by_session(tabs: &mut [Tab], kept: Carried) -> Option<Carried> {
    let index = kept
        .session
        .as_deref()
        .and_then(|session| session_tab(tabs, session));
    match index.and_then(|index| tabs.get_mut(index)) {
        Some(tab) => {
            give(tab, kept);
            None
        }
        None => Some(kept),
    }
}

/// Puts `kept` on the first chat with no history and no failure of its own.
fn place_in_empty_chat(tabs: &mut [Tab], kept: Carried) {
    if let Some(tab) = tabs
        .iter_mut()
        .find(|tab| tab.entries.is_empty() && tab.failure.is_none())
    {
        give(tab, kept);
    }
}

fn give(tab: &mut Tab, kept: Carried) {
    tab.failure = Some(ShownFailure::new(kept.failure));
    tab.last_turn = kept.last_turn.or(tab.last_turn.take());
}

#[cfg(test)]
mod tests {
    use subspace_agent::failure::TurnContext;
    use subspace_agent::{ChatEvent, Entry, Event, FailureCategory, TranscriptEntry, TurnFailure};

    use super::super::tabs::{LastTurn, Tab, TabState};
    use super::{ShownFailure, restore, take};

    fn failed(id: u64, session: Option<&str>, text: &str) -> Tab {
        let mut tab = Tab::new(id);
        tab.session = session.map(str::to_owned);
        tab.failure = Some(ShownFailure::new(TurnFailure::stopped(
            FailureCategory::Stopped,
            "fixture",
            &TurnContext::default(),
            None,
        )));
        tab.last_turn = Some(LastTurn {
            text: text.into(),
            context: Vec::new(),
            images: Vec::new(),
        });
        tab
    }

    /// The tabs a new runtime restored: the saved sessions with their
    /// history, then a fresh chat when `fresh` names one.
    fn restored(saved: &[&str], fresh: Option<&str>) -> Vec<Tab> {
        let mut tabs = vec![Tab::new(1)];
        let mut active = 0;
        let mut next = 9;
        let mut state = TabState {
            tabs: &mut tabs,
            active: &mut active,
            next_chat: &mut next,
        };
        let opened: Vec<(u64, String, bool)> = saved
            .iter()
            .map(|session| (*session, true))
            .chain(fresh.map(|session| (session, false)))
            .enumerate()
            .map(|(index, (session, saved))| (index as u64 + 1, session.to_owned(), saved))
            .collect();
        for (chat, session, saved) in &opened {
            let history = saved
                .then(|| TranscriptEntry {
                    id: format!("{session}-message"),
                    entry: Entry::User {
                        text: "earlier".into(),
                        steering: false,
                    },
                })
                .into_iter()
                .collect();
            state.receive(Event::of(*chat, ChatEvent::History(history)));
            state.receive(Event::of(
                *chat,
                ChatEvent::Ready {
                    session_id: session.clone(),
                },
            ));
        }
        state.receive(Event::Restore {
            tabs: opened
                .into_iter()
                .map(|(chat, session, _)| (chat, session))
                .collect(),
            active: 1,
        });
        tabs
    }

    fn texts(tabs: &[Tab]) -> Vec<Option<&str>> {
        tabs.iter()
            .map(|tab| tab.last_turn.as_ref().map(|turn| turn.text.as_str()))
            .collect()
    }

    #[test]
    fn a_failure_returns_to_the_restored_tab_of_its_session() {
        let mut old = vec![Tab::new(1), failed(2, Some("second"), "second message")];
        old[0].session = Some("first".into());
        let carried = take(&mut old);
        assert_eq!(carried.len(), 1, "only a failed tab is kept");
        assert!(old[1].failure.is_none());
        let mut tabs = restored(&["first", "second"], None);
        restore(&mut tabs, carried);
        assert!(tabs[0].failure.is_none(), "not the first tab");
        assert!(tabs[1].failure.is_some());
        assert_eq!(texts(&tabs), [None, Some("second message")]);
    }

    /// The first turn failed, so the chat was never saved: its failure goes
    /// to the new runtime's empty chat, where Retry sends it again.
    #[test]
    fn a_single_unsaved_failure_goes_to_the_new_empty_chat() {
        let carried = take(&mut [failed(3, Some("unsaved"), "Fix my config")]);
        let mut tabs = restored(&["first"], Some("fresh"));
        restore(&mut tabs, carried);
        assert!(tabs[0].failure.is_none(), "a saved chat is not used");
        assert!(tabs[1].failure.is_some());
        assert_eq!(texts(&tabs), [None, Some("Fix my config")]);
    }

    #[test]
    fn several_unsaved_failures_are_dropped() {
        let carried = take(&mut [failed(3, Some("gone"), "one"), failed(4, None, "two")]);
        let mut tabs = restored(&["first"], Some("fresh"));
        restore(&mut tabs, carried);
        assert!(tabs.iter().all(|tab| tab.failure.is_none()));
        assert!(tabs.iter().all(|tab| tab.last_turn.is_none()));
    }
}
