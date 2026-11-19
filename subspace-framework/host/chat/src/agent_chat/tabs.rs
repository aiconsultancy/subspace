//! One chat tab: everything a conversation keeps for itself, and the pure
//! rules for adding, choosing and closing tabs.

use std::collections::HashSet;

use gpui_kit::SharedString;
use subspace_agent::{
    ChatCommand, ChatEvent, ChatId, ChatMode, ContextRef, Event, ImageAttachment, MAX_CHATS,
    TranscriptEntry, Usage,
};

use super::tab_run::{ShownFailure, TabRun};
use super::{Pending, attention};
use crate::CloudSession;

/// A message typed before its chat was ready, with the context it carried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Queued {
    pub(super) text: String,
    pub(super) context: Vec<ContextRef>,
    pub(super) images: Vec<ImageAttachment>,
}

/// The last message a chat sent, with its context and images, so Retry can
/// send it again. It has the same parts as a queued message.
pub(super) type LastTurn = Queued;

impl Queued {
    /// The command that sends this message.
    pub(super) fn into_command(self) -> ChatCommand {
        let Self {
            text,
            context,
            images,
        } = self;
        ChatCommand::Send {
            text,
            context,
            images,
        }
    }
}

/// The title a tab shows before the runtime names it.
pub(super) const NEW_CHAT: &str = "New chat";

/// One open conversation. A turn in one tab continues while another is shown.
pub(super) struct Tab {
    pub(super) id: ChatId,
    /// The SDK session, once the runtime reported it.
    pub(super) session: Option<String>,
    pub(super) title: SharedString,
    pub(super) entries: Vec<TranscriptEntry>,
    pub(super) canvases: subspace_agent::canvas::CanvasList,
    pub(super) canvas_history:
        std::collections::HashMap<String, std::sync::Arc<subspace_agent::canvas::Canvas>>,
    pub(super) pending: Vec<Pending>,
    /// Request ids already surfaced for this open tab.
    pub(super) notified_requests: HashSet<String>,
    pub(super) context: Vec<ContextRef>,
    pub(super) images: Vec<ImageAttachment>,
    pub(super) expanded: HashSet<String>,
    /// What the user typed here but has not sent.
    pub(super) draft: String,
    /// A message typed while the runtime did not have this chat ready. It is
    /// sent once the chat is ready and idle, so nothing typed at start is
    /// lost.
    pub(super) queued: Option<Queued>,
    /// How many turns this tab has run, which seeds each turn's phrases.
    pub(super) turns: u64,
    pub(super) run: TabRun,
    pub(super) error: Option<String>,
    pub(super) errors: super::error_logging::AcceptedErrors,
    /// Why the last turn failed, until the next turn starts.
    pub(super) failure: Option<ShownFailure>,
    /// The message that started the last turn, which Retry sends again.
    /// Steering messages during the turn do not replace it.
    pub(super) last_turn: Option<LastTurn>,
    pub(super) usage: Usage,
    pub(super) yolo: bool,
    pub(super) confirm_yolo: bool,
    pub(super) model: Option<String>,
    pub(super) effort: Option<String>,
    pub(super) agent: Option<String>,
    pub(super) mode: ChatMode,
    /// The panel opened this tab itself, rather than adopting one the
    /// runtime restored, so a later restore keeps it.
    pub(super) opened_here: bool,
    /// The tab replays a cloud agent session: the input is disabled, the
    /// runtime rejects a message, and nothing here is saved.
    pub(super) read_only: bool,
    /// The replayed task's session page on GitHub, when its row had one.
    pub(super) session_url: Option<SharedString>,
}

impl Tab {
    pub(super) fn new(id: ChatId) -> Self {
        Self {
            id,
            session: None,
            title: NEW_CHAT.into(),
            entries: Vec::new(),
            canvases: Default::default(),
            canvas_history: Default::default(),
            pending: Vec::new(),
            notified_requests: HashSet::new(),
            context: Vec::new(),
            images: Vec::new(),
            expanded: HashSet::new(),
            draft: String::new(),
            queued: None,
            turns: 0,
            run: TabRun::Starting,
            error: None,
            errors: Default::default(),
            failure: None,
            last_turn: None,
            usage: Usage::default(),
            yolo: false,
            confirm_yolo: false,
            model: None,
            effort: None,
            agent: None,
            mode: ChatMode::Interactive,
            opened_here: false,
            read_only: false,
            session_url: None,
        }
    }

    /// A tab the panel opened, which no restore may drop.
    pub(super) fn opened(id: ChatId) -> Self {
        Self {
            opened_here: true,
            ..Self::new(id)
        }
    }

    /// A tab replaying a cloud agent task. It carries the task's name until
    /// the connected session reports its own.
    pub(super) fn replay(id: ChatId, task: &CloudSession) -> Self {
        Self {
            title: task.title.clone(),
            read_only: true,
            session_url: Some(task.url.clone()).filter(|url| !url.is_empty()),
            ..Self::opened(id)
        }
    }

    /// A background tab waits for an answer or an approval.
    pub(super) fn needs_attention(&self) -> bool {
        !self.pending.is_empty()
    }

    /// A tab the panel put there to have something to show, which no chat of
    /// the runtime has claimed. The runtime's first chat takes it over
    /// rather than leaving a dead tab beside a live one.
    pub(super) fn is_placeholder(&self) -> bool {
        !self.opened_here
            && self.session.is_none()
            && self.entries.is_empty()
            && self.queued.is_none()
    }
}

/// The narrowest a tab runs: a few characters, its markers and its close
/// button. Below this the strip scrolls instead.
const MIN_TAB: f32 = 96.;
/// How much wider the tab being shown is than the others.
const SELECTED_SHARE: f32 = 1.4;
/// A strip width the layout has not reported yet, where every tab takes its
/// minimum until the first measurement arrives.
const UNMEASURED: f32 = 0.;

/// The width of every tab in a strip of `available` pixels. The tabs fill
/// the strip: they share its width, the selected one takes a larger share,
/// and none runs narrower than the minimum, where the strip scrolls instead
/// of shrinking them further. There is no maximum, so one tab shows as much
/// of its title as the panel allows.
pub(super) fn tab_widths(available: f32, count: usize, selected: usize) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }
    let shares = (count - 1) as f32 + SELECTED_SHARE;
    let share = if available.is_finite() && available > UNMEASURED {
        available / shares
    } else {
        UNMEASURED
    };
    (0..count)
        .map(|index| {
            let wanted = if index == selected {
                share * SELECTED_SHARE
            } else {
                share
            };
            wanted.max(MIN_TAB)
        })
        .collect()
}

/// Where chat `id` sits, if it is still open.
pub(super) fn tab_index(tabs: &[Tab], id: ChatId) -> Option<usize> {
    tabs.iter().position(|tab| tab.id == id)
}

/// The tabs, the one shown and the next id the panel hands out. The runtime's
/// events act on this alone, so the rules are testable without a window.
pub(super) struct TabState<'a> {
    pub(super) tabs: &'a mut Vec<Tab>,
    pub(super) active: &'a mut usize,
    pub(super) next_chat: &'a mut ChatId,
}

impl TabState<'_> {
    /// Applies one runtime event, returning the transcript event the tab's
    /// rows still need.
    pub(super) fn receive(&mut self, event: Event) -> Option<(ChatId, attention::TabUpdate)> {
        match event {
            Event::Chat { chat, event } => {
                let index = self.tab_for(chat, &event)?;
                let tab = self.tabs.get_mut(index)?;
                Some((chat, attention::apply_chat_event(tab, event)))
            }
            Event::Restore { tabs, active } => {
                self.restore(tabs, active);
                None
            }
            _ => None,
        }
    }

    /// The runtime reopened the saved chats and numbered them. It reports
    /// that only once every chat is open, so its own events have already
    /// filled the tabs they belong to: the list is merged, never replaced,
    /// or a restored tab would lose the history and model it just received,
    /// and a tab opened while the runtime was starting would disappear.
    fn restore(&mut self, restored: Vec<(ChatId, String)>, active: ChatId) {
        if restored.is_empty() {
            return;
        }
        let restored_ids: Vec<ChatId> = restored.iter().map(|(id, _)| *id).collect();
        let mut merged: Vec<Tab> = Vec::new();
        for (id, session) in restored {
            let mut tab = self.take_tab(id).unwrap_or_else(|| Tab::new(id));
            tab.session = Some(session);
            merged.push(tab);
        }
        // Tabs the panel opened itself while the runtime was starting stay.
        merged.extend(
            self.tabs
                .drain(..)
                .filter(|tab| !restored_ids.contains(&tab.id) && tab.opened_here),
        );
        *self.tabs = merged;
        let highest = self.tabs.iter().map(|tab| tab.id).max().unwrap_or_default();
        *self.next_chat = (*self.next_chat).max(highest + 1);
        *self.active = tab_index(self.tabs, active).unwrap_or(0);
    }

    /// Adopt a restored chat when verified history arrives, before its title
    /// and remembered canvases. Ready also adopts chats with no history.
    fn tab_for(&mut self, chat: ChatId, event: &ChatEvent) -> Option<usize> {
        if let Some(index) = tab_index(self.tabs, chat) {
            return Some(index);
        }
        matches!(event, ChatEvent::History(_) | ChatEvent::Ready { .. }).then(|| self.adopt(chat))
    }

    /// Gives `chat` a tab: the placeholder the panel started with when it is
    /// still untouched, else a new tab at the end.
    fn adopt(&mut self, chat: ChatId) -> usize {
        *self.next_chat = (*self.next_chat).max(chat + 1);
        if let Some(index) = self.tabs.iter().position(Tab::is_placeholder) {
            self.tabs[index].id = chat;
            return index;
        }
        self.tabs.push(Tab::new(chat));
        self.tabs.len() - 1
    }

    /// The messages whose chats are ready, which the panel now sends. A
    /// queued message leaves the tab once, whether or not it is sent.
    pub(super) fn ready_to_send(&mut self) -> Vec<(ChatId, Queued)> {
        self.tabs
            .iter_mut()
            .filter(|tab| matches!(tab.run, TabRun::Idle) && tab.queued.is_some())
            .filter_map(|tab| tab.queued.take().map(|queued| (tab.id, queued)))
            .collect()
    }

    /// Removes the open tab with `id`, keeping everything it has received.
    fn take_tab(&mut self, id: ChatId) -> Option<Tab> {
        let index = tab_index(self.tabs, id)?;
        Some(self.tabs.remove(index))
    }
}

/// The tab showing `session`, if one does.
pub(super) fn session_tab(tabs: &[Tab], session: &str) -> Option<usize> {
    tabs.iter()
        .position(|tab| tab.session.as_deref() == Some(session))
}

/// The tab `step` places away, wrapping at both ends.
pub(super) fn stepped_index(count: usize, active: usize, step: isize) -> usize {
    if count == 0 {
        return 0;
    }
    let count = count as isize;
    let index = (active as isize + step).rem_euclid(count);
    index as usize
}

/// The tab that is shown after `closed` is removed: the one before it, so
/// closing from the right walks back along the strip.
pub(super) fn index_after_close(count: usize, closed: usize, active: usize) -> usize {
    let remaining = count.saturating_sub(1);
    if remaining == 0 {
        return 0;
    }
    let index = if active > closed { active - 1 } else { active };
    index.min(remaining - 1)
}

/// Whether another tab may be opened.
pub(super) fn can_open(count: usize) -> Result<(), String> {
    if count >= MAX_CHATS {
        return Err(format!(
            "At most {MAX_CHATS} chats are open at once. Close a tab first."
        ));
    }
    Ok(())
}

/// A saved session is reopened only when it is not already in a tab; the
/// panel focuses the existing tab instead.
pub(super) enum SessionChoice {
    Focus(usize),
    Open(String),
}

/// The saved chats already open, so the list can check them.
pub(super) fn choose_session(tabs: &[Tab], session: &str) -> SessionChoice {
    match session_tab(tabs, session) {
        Some(index) => SessionChoice::Focus(index),
        None => SessionChoice::Open(session.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SessionChoice, Tab, TabState, can_open, choose_session, index_after_close, session_tab,
        stepped_index, tab_index, tab_widths,
    };
    use subspace_agent::{ChatEvent, ChatId, ChatMode, Event, MAX_CHATS, TranscriptEntry};

    fn tabs(sessions: &[Option<&str>]) -> Vec<Tab> {
        sessions
            .iter()
            .enumerate()
            .map(|(index, session)| Tab {
                session: session.map(str::to_owned),
                ..Tab::new(index as u64 + 1)
            })
            .collect()
    }

    #[test]
    fn a_tab_is_found_by_its_chat_and_by_its_session() {
        let tabs = tabs(&[Some("a"), None, Some("c")]);
        assert_eq!(tab_index(&tabs, 3), Some(2));
        assert_eq!(tab_index(&tabs, 9), None);
        assert_eq!(session_tab(&tabs, "c"), Some(2));
        assert_eq!(session_tab(&tabs, "gone"), None);
    }

    #[test]
    fn next_and_previous_wrap_around_the_strip() {
        assert_eq!(stepped_index(3, 2, 1), 0);
        assert_eq!(stepped_index(3, 0, -1), 2);
        assert_eq!(stepped_index(3, 1, 1), 2);
        assert_eq!(stepped_index(1, 0, 1), 0);
        assert_eq!(stepped_index(0, 0, 1), 0, "no tabs, no movement");
    }

    #[test]
    fn closing_a_tab_keeps_a_sensible_tab_shown() {
        assert_eq!(index_after_close(3, 2, 2), 1, "the last tab steps back");
        assert_eq!(index_after_close(3, 0, 2), 1, "a tab before it shifts down");
        assert_eq!(index_after_close(3, 2, 0), 0, "a later tab leaves it alone");
        assert_eq!(index_after_close(1, 0, 0), 0, "the last tab leaves none");
    }

    #[test]
    fn the_ninth_tab_is_refused_with_an_explanation() {
        assert!(can_open(0).is_ok());
        assert!(can_open(MAX_CHATS - 1).is_ok());
        let refused = can_open(MAX_CHATS).unwrap_err();
        assert!(refused.contains("Close a tab"));
    }

    #[test]
    fn choosing_a_session_focuses_its_tab_and_otherwise_opens_one() {
        let tabs = tabs(&[Some("a"), Some("b")]);
        assert!(matches!(
            choose_session(&tabs, "b"),
            SessionChoice::Focus(1)
        ));
        assert!(matches!(
            choose_session(&tabs, "c"),
            SessionChoice::Open(id) if id == "c"
        ));
    }

    #[test]
    fn one_tab_fills_the_whole_strip() {
        assert_eq!(tab_widths(640., 1, 0), vec![640.]);
        assert_eq!(
            tab_widths(1200., 1, 0),
            vec![1200.],
            "a wide panel shows as much of the title as it can"
        );
    }

    #[test]
    fn tabs_share_the_strip_and_the_shown_one_takes_more() {
        let widths = tab_widths(500., 3, 1);
        assert_eq!(widths.len(), 3);
        assert!(widths[1] > widths[0], "the selected tab is wider");
        assert!((widths[1] / widths[0] - 1.4).abs() < 0.01);
        assert!(
            (widths.iter().sum::<f32>() - 500.).abs() < 0.01,
            "they fill the strip: {widths:?}"
        );
    }

    #[test]
    fn two_tabs_share_the_strip_in_the_selected_tabs_favour() {
        let widths = tab_widths(480., 2, 0);
        assert!((widths[0] / widths[1] - 1.4).abs() < 0.01, "{widths:?}");
        assert!(
            (widths.iter().sum::<f32>() - 480.).abs() < 0.01,
            "{widths:?}"
        );
    }

    #[test]
    fn adding_a_tab_shrinks_the_others_and_closing_gives_the_space_back() {
        let three = tab_widths(500., 3, 0);
        let four = tab_widths(500., 4, 0);
        assert!(four[1] < three[1], "{three:?} then {four:?}");
        assert_eq!(tab_widths(500., 3, 0), three);
    }

    #[test]
    fn many_tabs_stop_at_the_minimum_and_the_strip_scrolls() {
        let crowded = tab_widths(200., 8, 0);
        assert!(
            crowded.iter().all(|width| (*width - 96.).abs() < 0.01),
            "the strip scrolls instead of shrinking further: {crowded:?}"
        );
        assert!(
            crowded.iter().sum::<f32>() > 200.,
            "they overflow the strip, which is what makes it scroll"
        );
    }

    #[test]
    fn an_unmeasured_strip_still_gives_every_tab_a_width() {
        assert_eq!(tab_widths(0., 2, 0), vec![96., 96.]);
        assert_eq!(tab_widths(f32::NAN, 1, 0), vec![96.]);
        assert!(tab_widths(900., 0, 0).is_empty());
    }

    #[test]
    fn a_tab_with_a_question_asks_for_attention() {
        let mut tab = Tab::new(1);
        assert!(!tab.needs_attention());
        tab.pending.push(super::Pending::Approval {
            id: "one".into(),
            summary: "write".into(),
        });
        assert!(tab.needs_attention());
    }

    /// The events the actor pushes while it reopens one saved chat. The
    /// chat's own events come first, because the runtime reports which tabs
    /// it restored only once every one of them is open.
    fn startup_events() -> Vec<Event> {
        vec![
            Event::of(1, ChatEvent::History(vec![entry("Check the risks")])),
            Event::of(1, ChatEvent::Title("Release risks".into())),
            Event::of(1, ChatEvent::Mode(false)),
            Event::of(
                1,
                ChatEvent::Ready {
                    session_id: "1a6987f5".into(),
                },
            ),
            Event::of(
                1,
                ChatEvent::SelectedModel {
                    model: "claude-sonnet-4.5".into(),
                    effort: None,
                },
            ),
            Event::of(1, ChatEvent::SelectedMode(ChatMode::Interactive)),
            Event::of(1, ChatEvent::SelectedAgent("subspace".into())),
            Event::Restore {
                tabs: vec![(1, "1a6987f5".to_owned())],
                active: 1,
            },
        ]
    }

    fn entry(text: &str) -> TranscriptEntry {
        TranscriptEntry {
            id: "1".into(),
            entry: subspace_agent::Entry::User {
                text: text.into(),
                steering: false,
            },
        }
    }

    /// The state the panel hands the runtime's events, over three locals.
    fn state<'a>(
        tabs: &'a mut Vec<Tab>,
        active: &'a mut usize,
        next: &'a mut ChatId,
    ) -> TabState<'a> {
        TabState {
            tabs,
            active,
            next_chat: next,
        }
    }

    /// A panel after the runtime's events: the tabs it had, the tab shown
    /// and the next id it would hand out.
    fn after(
        tabs: Vec<Tab>,
        active: usize,
        next: ChatId,
        events: Vec<Event>,
    ) -> (Vec<Tab>, usize, ChatId) {
        let (mut tabs, mut active, mut next) = (tabs, active, next);
        feed(&mut tabs, &mut active, &mut next, events);
        (tabs, active, next)
    }

    /// The panel as it starts: one placeholder tab, and its own ids starting
    /// past the range the runtime restores into.
    fn started(events: Vec<Event>) -> (Vec<Tab>, usize, ChatId) {
        after(vec![Tab::new(1)], 0, 9, events)
    }

    fn feed(tabs: &mut Vec<Tab>, active: &mut usize, next: &mut ChatId, events: Vec<Event>) {
        for event in events {
            state(tabs, active, next).receive(event);
        }
    }

    #[test]
    fn the_restored_tab_keeps_what_the_runtime_already_sent_it() {
        let mut tabs = vec![Tab::new(1)];
        let (mut active, mut next) = (0, 9);
        feed(&mut tabs, &mut active, &mut next, startup_events());
        assert_eq!(tabs.len(), 1);
        let tab = &tabs[0];
        assert!(
            tab.is_ready(),
            "the tab the runtime reported ready is ready"
        );
        assert_eq!(tab.title, "Release risks");
        assert_eq!(tab.entries.len(), 1, "its replayed history is still there");
        assert_eq!(tab.model.as_deref(), Some("claude-sonnet-4.5"));
        assert_eq!(tab.session.as_deref(), Some("1a6987f5"));
        assert_eq!(active, 0);
    }

    #[test]
    fn a_second_restored_tab_keeps_history_and_title_sent_before_ready() {
        let mut events = startup_events();
        events.extend([
            Event::of(2, ChatEvent::History(vec![entry("Draw the architecture")])),
            Event::of(2, ChatEvent::Title("Architecture canvases".into())),
            Event::of(
                2,
                ChatEvent::Ready {
                    session_id: "second".into(),
                },
            ),
            Event::Restore {
                tabs: vec![(1, "first".into()), (2, "second".into())],
                active: 2,
            },
        ]);
        let (tabs, active, _) = started(events);
        assert_eq!(active, 1);
        assert_eq!(tabs[1].title, "Architecture canvases");
        assert_eq!(tabs[1].entries.len(), 1);
        assert!(tabs[1].is_ready());
    }

    #[test]
    fn a_tab_opened_while_the_runtime_started_survives_the_restore() {
        // The user presses + before the runtime has finished starting.
        let (tabs, active, next) =
            after(vec![Tab::new(1), Tab::opened(9)], 1, 10, startup_events());
        let ids: Vec<ChatId> = tabs.iter().map(|tab| tab.id).collect();
        assert_eq!(ids, [1, 9], "the new tab is kept beside the restored one");
        assert_eq!(active, 0, "the restore selects the chat it reported");
        assert!(tabs[0].is_ready());
        assert_eq!(next, 10, "the panel keeps numbering past its own tabs");
    }

    #[test]
    fn the_runtime_fills_a_tab_the_panel_opened() {
        let (tabs, _active, _next) = after(
            vec![Tab::new(1), Tab::opened(9)],
            1,
            10,
            vec![
                Event::of(9, ChatEvent::History(Vec::new())),
                Event::of(
                    9,
                    ChatEvent::Ready {
                        session_id: "a6dca147".into(),
                    },
                ),
                Event::of(
                    9,
                    ChatEvent::SelectedModel {
                        model: "gpt-5".into(),
                        effort: Some("high".into()),
                    },
                ),
            ],
        );
        let opened = &tabs[1];
        assert!(opened.is_ready(), "Enter can send in the tab + opened");
        assert_eq!(opened.session.as_deref(), Some("a6dca147"));
        assert_eq!(opened.model.as_deref(), Some("gpt-5"));
        assert_eq!(opened.effort.as_deref(), Some("high"));
    }

    #[test]
    fn restoring_several_tabs_numbers_the_next_one_after_them() {
        let (tabs, active, next) = started(vec![Event::Restore {
            tabs: vec![(1, "one".to_owned()), (2, "two".to_owned())],
            active: 2,
        }]);
        assert_eq!(tabs.len(), 2);
        assert_eq!(active, 1, "the saved active tab is shown");
        assert_eq!(tabs[1].session.as_deref(), Some("two"));
        assert_eq!(next, 9);
    }

    /// With nothing saved, the runtime opens a fresh chat and reports it in
    /// `Restore`, but its `Ready` arrives first. The placeholder the panel
    /// started with takes that chat, so Enter works in the tab the user sees.
    #[test]
    fn the_placeholder_becomes_the_chat_the_runtime_opened() {
        let (tabs, _active, _next) = started(vec![
            Event::of(1, ChatEvent::History(Vec::new())),
            Event::of(
                1,
                ChatEvent::Ready {
                    session_id: "fresh".into(),
                },
            ),
            Event::Restore {
                tabs: vec![(1, "fresh".to_owned())],
                active: 1,
            },
        ]);
        assert_eq!(tabs.len(), 1);
        assert!(tabs[0].is_ready(), "the tab the user types in can send");
        assert_eq!(tabs[0].session.as_deref(), Some("fresh"));
    }

    /// A saved chat that no longer resumes is dropped, so the chat the
    /// runtime opens in its place is numbered past the placeholder's id. Its
    /// `Ready` still has to reach a tab.
    #[test]
    fn a_chat_numbered_past_the_placeholder_still_takes_it_over() {
        let (tabs, active, next) = started(vec![
            Event::Error("Cannot resume the saved SDK session.".into()),
            Event::of(2, ChatEvent::History(Vec::new())),
            Event::of(
                2,
                ChatEvent::Ready {
                    session_id: "replacement".into(),
                },
            ),
            Event::of(
                2,
                ChatEvent::SelectedModel {
                    model: "gpt-5".into(),
                    effort: None,
                },
            ),
            Event::Restore {
                tabs: vec![(2, "replacement".to_owned())],
                active: 2,
            },
        ]);
        let ids: Vec<ChatId> = tabs.iter().map(|tab| tab.id).collect();
        assert_eq!(ids, [2], "no dead tab is left beside it");
        assert_eq!(tabs[0].id, 2);
        assert!(tabs[0].is_ready());
        assert_eq!(tabs[0].model.as_deref(), Some("gpt-5"));
        assert_eq!(active, 0);
        assert_eq!(next, 9, "the panel keeps numbering past its own range");
    }

    /// Two restored chats, whose events both arrive before `Restore`.
    #[test]
    fn a_second_restored_chat_gets_its_own_tab() {
        let (tabs, active, _next) = started(vec![
            Event::of(
                1,
                ChatEvent::Ready {
                    session_id: "one".into(),
                },
            ),
            Event::of(
                2,
                ChatEvent::Ready {
                    session_id: "two".into(),
                },
            ),
            Event::Restore {
                tabs: vec![(1, "one".to_owned()), (2, "two".to_owned())],
                active: 2,
            },
        ]);
        let ids: Vec<ChatId> = tabs.iter().map(|tab| tab.id).collect();
        assert_eq!(ids, [1, 2]);
        assert!(tabs.iter().all(Tab::is_ready), "both can send");
        assert_eq!(active, 1, "the saved active tab is shown");
    }

    /// A tab the user opened is never taken over by the runtime's own
    /// chat, however the ids fall.
    #[test]
    fn a_tab_zack_opened_is_not_taken_over() {
        let (tabs, _active, _next) = after(
            vec![Tab::opened(9)],
            0,
            10,
            vec![Event::of(
                1,
                ChatEvent::Ready {
                    session_id: "restored".into(),
                },
            )],
        );
        let ids: Vec<ChatId> = tabs.iter().map(|tab| tab.id).collect();
        assert_eq!(ids, [9, 1], "the restored chat gets its own tab");
        assert!(!tabs[0].is_ready());
        assert!(tabs[1].is_ready());
    }

    #[test]
    fn a_message_typed_before_a_chat_is_ready_is_sent_once_it_is() {
        let mut tabs = vec![Tab::new(1)];
        let (mut active, mut next) = (0, 9);
        tabs[0].queued = Some(super::Queued {
            text: "count slowly to three".into(),
            context: Vec::new(),
            images: Vec::new(),
        });
        let mut state = state(&mut tabs, &mut active, &mut next);
        assert!(
            state.ready_to_send().is_empty(),
            "nothing goes before the chat is ready"
        );
        state.receive(Event::of(
            1,
            ChatEvent::Ready {
                session_id: "fresh".into(),
            },
        ));
        let sending = state.ready_to_send();
        assert_eq!(sending.len(), 1);
        assert_eq!(sending[0].0, 1);
        assert_eq!(sending[0].1.text, "count slowly to three");
        assert!(
            state.ready_to_send().is_empty(),
            "a queued message is sent once"
        );
    }

    #[test]
    fn a_busy_chat_keeps_its_queued_message_until_the_turn_ends() {
        let mut tabs = vec![Tab::new(1)];
        tabs[0].begin();
        tabs[0].queued = Some(super::Queued {
            text: "later".into(),
            context: Vec::new(),
            images: Vec::new(),
        });
        let (mut active, mut next) = (0, 9);
        let mut state = state(&mut tabs, &mut active, &mut next);
        assert!(state.ready_to_send().is_empty());
        state.receive(Event::of(1, ChatEvent::TurnDone));
        assert_eq!(state.ready_to_send().len(), 1);
    }

    #[test]
    fn a_placeholder_is_only_a_tab_nobody_has_used() {
        assert!(Tab::new(1).is_placeholder());
        assert!(!Tab::opened(9).is_placeholder(), "the user opened this one");
        let mut used = Tab::new(1);
        used.session = Some("one".into());
        assert!(!used.is_placeholder(), "the runtime claimed this one");
        let mut waiting = Tab::new(1);
        waiting.queued = Some(super::Queued {
            text: "hello".into(),
            context: Vec::new(),
            images: Vec::new(),
        });
        assert!(!waiting.is_placeholder(), "a message waits in this one");
    }

    #[test]
    fn a_replay_tab_is_read_only_and_keeps_the_way_back_to_its_page() {
        let task = super::CloudSession {
            task_id: "3f0a".into(),
            title: "Fix the flaky test".into(),
            url: "https://github.com/copilot/agents/3f0a".into(),
        };
        let tab = Tab::replay(4, &task);
        assert!(tab.read_only);
        assert_eq!(tab.title, "Fix the flaky test");
        assert_eq!(
            tab.session_url.as_deref(),
            Some("https://github.com/copilot/agents/3f0a")
        );
        assert!(tab.opened_here, "no restore may drop it");
        assert!(!Tab::new(1).read_only, "an ordinary chat still sends");
    }

    #[test]
    fn a_replay_without_a_page_offers_no_link() {
        let task = super::CloudSession {
            task_id: "3f0a".into(),
            title: "Fix the flaky test".into(),
            url: "".into(),
        };
        assert_eq!(Tab::replay(4, &task).session_url, None);
    }

    #[test]
    fn a_turn_seeds_its_own_status_phrases() {
        let mut tab = Tab::new(1);
        tab.begin();
        let first = tab
            .running()
            .copied()
            .expect("the turn is running and timed");
        assert!(first.notify, "a turn started here alerts");
        tab.end();
        tab.begin();
        let second = tab.running().copied().expect("a new turn");
        assert_ne!(second.seed, first.seed, "each turn gets its own seed");
        assert_ne!(second.phrase, first.phrase, "so it opens on another phrase");
    }

    #[test]
    fn a_steering_message_keeps_the_running_turn() {
        let mut tab = Tab::new(1);
        tab.begin();
        let running = tab.running().copied();
        tab.error = Some("earlier failure".into());
        tab.begin();
        assert_eq!(
            tab.running().copied(),
            running,
            "the turn it joins keeps its seed, status phrase and timer"
        );
        assert_eq!(tab.error, None, "a sent message clears the last error");
    }

    #[test]
    fn an_event_for_a_closed_chat_changes_nothing() {
        let (tabs, _active, _next) = started(vec![Event::of(7, ChatEvent::TurnDone)]);
        assert!(!tabs[0].is_busy());
        assert_eq!(tabs.len(), 1, "only a ready chat is adopted");
    }

    fn delta() -> ChatEvent {
        ChatEvent::Delta {
            kind: subspace_agent::EntryKind::Assistant,
            text: "working ".into(),
        }
    }

    fn ready(session: &str) -> ChatEvent {
        ChatEvent::Ready {
            session_id: session.into(),
        }
    }

    #[test]
    fn a_turn_in_a_background_tab_marks_only_that_tab() {
        let (tabs, _active, _next) = after(
            vec![Tab::new(1), Tab::opened(9)],
            0,
            10,
            vec![
                Event::of(9, ready("background")),
                Event::of(9, delta()),
                Event::of(
                    9,
                    ChatEvent::Approval {
                        request_id: "write-1".into(),
                        summary: "Update task".into(),
                    },
                ),
            ],
        );
        assert!(tabs[1].is_busy());
        assert!(tabs[1].needs_attention());
        assert!(!tabs[0].is_busy());
        assert!(!tabs[0].needs_attention());
    }

    /// Output for a chat that is idle here, as for a turn this panel did not
    /// start, runs a timed turn so the status line moves.
    #[test]
    fn a_delta_while_idle_starts_a_timed_turn() {
        let (tabs, _active, _next) =
            started(vec![Event::of(1, ready("one")), Event::of(1, delta())]);
        let turn = tabs[0].running().copied().expect("the turn runs");
        assert!(!turn.notify, "a turn started elsewhere does not alert");
        assert_eq!(turn.seed, tabs[0].turns, "it has its own seed");
    }

    /// An error the runtime reports during a turn, such as a refused steering
    /// message or a canvas that did not open, does not end the turn: it keeps
    /// its timer and requests, and the next message steers it.
    #[test]
    fn an_error_during_a_turn_keeps_it_running() {
        let mut tabs = vec![Tab::new(1)];
        let (mut active, mut next) = (0, 9);
        state(&mut tabs, &mut active, &mut next).receive(Event::of(1, ready("one")));
        tabs[0].begin();
        tabs[0].pending.push(super::Pending::Approval {
            id: "write-1".into(),
            summary: "Update task".into(),
        });
        let running = tabs[0].running().copied();
        for event in [ChatEvent::Error("refused".into()), delta()] {
            state(&mut tabs, &mut active, &mut next).receive(Event::of(1, event));
        }
        assert_eq!(tabs[0].running().copied(), running, "same turn, same timer");
        assert_eq!(tabs[0].error.as_deref(), Some("refused"));
        assert_eq!(tabs[0].pending.len(), 1, "its request is still open");
        let done = state(&mut tabs, &mut active, &mut next)
            .receive(Event::of(1, ChatEvent::TurnDone))
            .and_then(|(_, update)| update.attention);
        assert_eq!(done, None, "a turn that showed an error does not alert");
        assert!(tabs[0].pending.is_empty());
    }

    /// Every way a turn ends clears its approval cards.
    #[test]
    fn every_turn_end_clears_pending_requests() {
        use subspace_agent::failure::TurnContext;
        use subspace_agent::{FailureCategory, TurnFailure};
        let failure = |category| {
            ChatEvent::Failure(Box::new(TurnFailure::stopped(
                category,
                "fixture",
                &TurnContext::default(),
                None,
            )))
        };
        let ends = [
            vec![ChatEvent::TurnDone],
            vec![failure(FailureCategory::Stopped)],
            vec![failure(FailureCategory::NotResponding)],
            vec![failure(FailureCategory::NoResponse), ChatEvent::TurnDone],
        ];
        for events in ends {
            let mut tab = Tab::new(1);
            tab.ready();
            tab.begin();
            tab.pending.push(super::Pending::Approval {
                id: "write-1".into(),
                summary: "Update task".into(),
            });
            for event in events {
                super::attention::apply_chat_event(&mut tab, event);
            }
            assert!(!tab.is_busy());
            assert!(tab.pending.is_empty());
        }
        let mut stopped = vec![Tab::new(1)];
        stopped[0].begin();
        stopped[0].pending.push(super::Pending::Approval {
            id: "write-1".into(),
            summary: "Update task".into(),
        });
        assert!(super::attention::end_turns(&mut stopped, true));
        assert!(stopped[0].pending.is_empty() && !stopped[0].is_ready());
    }

    /// A failed steering message leaves the turn to its own end.
    #[test]
    fn a_failure_that_needs_no_reconnect_waits_for_the_turn_to_end() {
        use subspace_agent::failure::TurnContext;
        use subspace_agent::{FailureCategory, TurnFailure};
        let mut tab = Tab::new(1);
        tab.ready();
        tab.begin();
        super::attention::apply_chat_event(
            &mut tab,
            ChatEvent::Failure(Box::new(TurnFailure::stopped(
                FailureCategory::NoResponse,
                "fixture",
                &TurnContext::default(),
                None,
            ))),
        );
        assert!(tab.is_busy(), "the turn goes on");
        assert!(tab.failure.is_some());
    }

    #[test]
    fn a_transcript_event_is_handed_back_for_its_own_tab() {
        let mut tabs = vec![Tab::new(1)];
        let (mut active, mut next) = (0, 9);
        let handed = state(&mut tabs, &mut active, &mut next)
            .receive(Event::of(1, ChatEvent::Entry(entry("hello"))));
        assert!(matches!(
            handed.and_then(|(_, update)| update.transcript),
            Some(ChatEvent::Entry(_))
        ));
    }

    #[test]
    fn live_turn_completion_is_selected_once_but_restore_and_failure_are_silent() {
        let mut tabs = vec![Tab::new(1), Tab::opened(9)];
        tabs[0].ready();
        tabs[1].ready();
        tabs[1].begin();
        let (mut active, mut next) = (0, 10);
        let mut receive = |tabs: &mut Vec<Tab>, chat, event| {
            state(tabs, &mut active, &mut next)
                .receive(Event::of(chat, event))
                .and_then(|(_, update)| update.attention)
        };
        let first = receive(&mut tabs, 9, ChatEvent::TurnDone);
        let duplicate = receive(&mut tabs, 9, ChatEvent::TurnDone);
        assert_eq!(first, Some(super::super::ChatAttentionKind::TurnCompleted));
        assert_eq!(duplicate, None);

        receive(&mut tabs, 1, delta());
        let restored = receive(&mut tabs, 1, ChatEvent::TurnDone);
        tabs[1].begin();
        receive(&mut tabs, 9, ChatEvent::Error("failed".into()));
        let failed = receive(&mut tabs, 9, ChatEvent::TurnDone);
        tabs[1].begin();
        tabs[1].cancel_completion_alert();
        let cancelled = receive(&mut tabs, 9, ChatEvent::TurnDone);
        assert_eq!((restored, failed, cancelled), (None, None, None));
    }

    #[test]
    fn a_live_request_is_selected_once_even_in_a_background_tab() {
        let mut tabs = vec![Tab::new(1), Tab::opened(9)];
        tabs[1].begin();
        let (mut active, mut next) = (0, 10);
        let request = || ChatEvent::Approval {
            request_id: "write-1".into(),
            summary: "sensitive summary".into(),
        };
        let first = state(&mut tabs, &mut active, &mut next)
            .receive(Event::of(9, request()))
            .and_then(|(_, update)| update.attention);
        let duplicate = state(&mut tabs, &mut active, &mut next)
            .receive(Event::of(9, request()))
            .and_then(|(_, update)| update.attention);
        let question = state(&mut tabs, &mut active, &mut next)
            .receive(Event::of(
                9,
                ChatEvent::Question {
                    request_id: "question-1".into(),
                    question: "sensitive prompt".into(),
                    choices: vec!["secret choice".into()],
                },
            ))
            .and_then(|(_, update)| update.attention);
        assert_eq!(
            first,
            Some(super::super::ChatAttentionKind::WaitingForInput)
        );
        assert_eq!(duplicate, None);
        assert_eq!(
            question,
            Some(super::super::ChatAttentionKind::WaitingForInput)
        );
        assert_eq!(
            tabs[1].pending.len(),
            2,
            "duplicate requests are not shown twice"
        );
    }
}
