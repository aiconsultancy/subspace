//! One buffer's conversation, self-contained so any host can show it: the
//! board's detail panel today, a dock panel later. It owns the buffer's
//! scroll position and composer draft, follows the hub itself, and tells its
//! host about navigation through [`ConversationEvent`]s.

pub(crate) mod find;
mod header;
mod keys;
mod panel;
pub(crate) mod summaries;

pub(crate) use keys::{KEYS, bind_keys};
#[cfg(test)]
pub(crate) use keys::{focused, focused_upload, has_card};

use std::{rc::Rc, sync::Arc};

use chrono::Local;
use gpui_kit::component::dock::TabGroup;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, Subscription, TestSupportExt as _,
    WeakEntity, Window, div,
};
use subspace_client_irc::{BufferKey, Command, Target};
use subspace_framework::contracts::github::recognition::PullRequestLink;
#[cfg(test)]
use subspace_framework::dashboard::ChatInput;
use subspace_framework::dashboard::Tokens;

use crate::compose::{self, Draft};
use crate::hub::Hub;
use crate::markup::{Mentions, Ref};
use crate::transcript::{self, Claim, Row, RowContext, Scroller};

/// What a conversation asks its host to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConversationEvent {
    /// A `#channel` or member reference was chosen.
    Open(Ref),
    /// Focus moved into the conversation, such as its composer.
    Focused,
    /// Its dock panel was closed.
    Closed,
    /// The displayed local-find document changed.
    FindScopeChanged,
}

pub(crate) struct Conversation {
    me: WeakEntity<Self>,
    pub(crate) buffer: BufferKey,
    pub(crate) hub: Entity<Hub>,
    scroll: Scroller,
    pub(crate) summaries: summaries::Summaries,
    replies: Rc<transcript::replies::Counts>,
    pub(crate) draft: Draft,
    pub(crate) tray: crate::uploads::outgoing::Tray,
    pub(crate) upload_viewer: Option<crate::uploads::viewer::Viewer>,
    pub(crate) find_active: bool,
    pub(crate) find_cache: std::cell::RefCell<find::Cache>,
    find_source: Option<find::Revision>,
    focus: FocusHandle,
    /// The dock tab group holding this conversation, whose tab shows the
    /// unread count.
    group: Option<WeakEntity<TabGroup>>,
    /// The content the tab last showed.
    shown_tab: panel::TabContent,
    /// The line a search result opened, shown selected.
    target: Option<i64>,
    /// The transcript's keyboard focus.
    keyboard: keys::Keyboard,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ConversationEvent> for Conversation {}

impl Conversation {
    /// The conversation of `buffer`, showing its latest lines.
    pub fn new(
        buffer: BufferKey,
        hub: Entity<Hub>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = Draft::new(&buffer, window, cx);
        let focus = cx.focus_handle();
        let keyboard = keys::Keyboard::new(cx);
        let subscriptions = vec![
            cx.on_blur(&keyboard.handle, window, |this, _, cx| {
                keys::blurred(this, cx)
            }),
            cx.on_focus_in(&focus, window, |_, _, cx| {
                cx.emit(ConversationEvent::Focused)
            }),
            cx.observe(&hub, |this: &mut Self, _, cx| this.sync(cx)),
            cx.subscribe_in(&draft.entity, window, |this, _, event, window, cx| {
                compose::handle(this, event, window, cx);
            }),
        ];
        let mut conversation = Self {
            me: cx.entity().downgrade(),
            buffer,
            hub,
            scroll: Scroller::new(cx),
            summaries: Default::default(),
            replies: Rc::default(),
            draft,
            tray: crate::uploads::outgoing::Tray::default(),
            upload_viewer: None,
            find_active: false,
            find_cache: Default::default(),
            find_source: None,
            focus,
            group: None,
            shown_tab: Default::default(),
            target: None,
            keyboard,
            _subscriptions: subscriptions,
        };
        conversation.show_latest(cx);
        conversation
    }

    #[cfg(test)]
    pub fn composer(&self) -> &Entity<ChatInput> {
        &self.draft.entity
    }

    /// Puts the keyboard in the composer.
    pub fn focus_composer(&self, window: &mut Window, cx: &mut App) {
        self.draft
            .entity
            .update(cx, |input, cx| input.focus(window, cx));
    }

    pub fn send(&self, command: Command, cx: &mut App) {
        self.hub.update(cx, |hub, _| hub.send(command));
    }

    /// The board's measured height, which caps the composer's.
    pub fn set_available_height(&self, available: f32, cx: &mut App) {
        self.draft
            .entity
            .update(cx, |input, cx| input.set_available_height(available, cx));
    }

    /// The rows and their identities as the hub holds the buffer now.
    fn current_rows(&self, cx: &App) -> (Vec<Row>, Vec<String>) {
        let state = &self.hub.read(cx).state;
        let Some(held) = state.buffer(&self.buffer) else {
            return (Vec::new(), Vec::new());
        };
        let rows = transcript::rows(held, &self.buffer.target, &Local);
        let ids = transcript::identities(&rows, &held.entries);
        (rows, ids)
    }

    /// Shows the buffer from its latest line.
    fn show_latest(&mut self, cx: &mut Context<Self>) {
        self.refresh_replies(cx);
        self.prune_summaries(cx);
        let (rows, ids) = self.current_rows(cx);
        self.scroll.reset(rows, ids, cx);
    }

    /// Scrolls to the line numbered `seq` and selects it, as opening a
    /// search result does. False when the buffer does not hold it.
    pub fn reveal(&mut self, seq: i64, cx: &mut Context<Self>) -> bool {
        let (rows, ids) = self.current_rows(cx);
        let at = self
            .hub
            .read(cx)
            .state
            .buffer(&self.buffer)
            .and_then(|held| held.entries.iter().position(|entry| entry.seq == seq));
        let index = at.and_then(|at| transcript::row_of(&rows, at));
        self.scroll.replace(rows, ids, cx);
        let Some(index) = index else {
            return false;
        };
        self.target = Some(seq);
        self.scroll
            .state
            .update(cx, |state, cx| _ = state.scroll_to_item(index, cx));
        cx.notify();
        true
    }

    /// The line a search result opened, if any.
    #[cfg(test)]
    pub fn target(&self) -> Option<i64> {
        self.target
    }

    /// Re-reads the buffer after the hub changed, keeping the reader's place.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let previous = self.replies.clone();
        self.refresh_replies(cx);
        self.prune_summaries(cx);
        let (rows, ids) = self.current_rows(cx);
        let changed = self.changed_reply_rows(&rows, &previous, cx);
        let revision = self.find_active.then(|| find::revision(self, cx));
        let find_changed = self.scroll.rows() != rows || self.find_source != revision;
        self.find_source = revision;
        self.scroll.replace(rows, ids, cx);
        self.scroll.remeasure(&changed, cx);
        if find_changed {
            cx.emit(ConversationEvent::FindScopeChanged);
        }
        self.refresh_tab(cx);
        compose::revalidate(self, cx);
        crate::uploads::outgoing::follow(self, cx);
        crate::uploads::viewer::follow(self, cx);
        cx.notify();
    }

    fn changed_reply_rows(
        &self,
        rows: &[Row],
        previous: &transcript::replies::Counts,
        cx: &App,
    ) -> Vec<usize> {
        self.hub
            .read(cx)
            .state
            .buffer(&self.buffer)
            .map(|held| {
                transcript::replies::changed_rows(rows, &held.entries, previous, &self.replies)
            })
            .unwrap_or_default()
    }

    fn refresh_replies(&mut self, cx: &App) {
        let counts = self
            .hub
            .read(cx)
            .state
            .buffer(&self.buffer)
            .map(|held| transcript::replies::counts(&self.buffer, held))
            .unwrap_or_default();
        self.replies = Rc::new(counts);
    }

    /// The shared row facts: who may be mentioned, today, and where
    /// reference clicks go.
    fn row_context(&self, cx: &App) -> RowContext {
        RowContext {
            conversation: self.me.clone(),
            mentions: mentions(self, cx),
            today: Local::now().date_naive(),
            claim: claim(self, cx),
            upload: crate::uploads::viewer::claim(&self.me),
            target: self.target,
            hub: self.hub.downgrade(),
            focus: self.keyboard.at,
            card: self.keyboard.at.and(self.keyboard.card.clone()),
            upload_focus: keys::focused_upload(self, cx),
            replies: self.replies.clone(),
        }
    }

    fn row_at(&self, index: usize, cx: &App) -> (AnyElement, Vec<String>) {
        let Some(row) = self.scroll.row(index) else {
            return (div().into_any_element(), Vec::new());
        };
        let context = self.row_context(cx);
        let state = &self.hub.read(cx).state;
        let Some(held) = state.buffer(&self.buffer) else {
            return (div().into_any_element(), Vec::new());
        };
        transcript::row(row, held, (&self.hub, &self.buffer), &context, cx)
    }

    fn header_facts(&self, cx: &App) -> Option<header::HeaderFacts> {
        let state = &self.hub.read(cx).state;
        let held = state.buffer(&self.buffer);
        let network = state.network(&self.buffer.network)?;
        let topic = held
            .and_then(|held| held.topic.as_ref())
            .map(|topic| subspace_client_irc::format::strip(&topic.text));
        let (title, context) = match &self.buffer.target {
            Target::Server => (network.name.clone(), crate::view::server_summary(network)),
            Target::Query(name) | Target::Channel(name) => (name.clone(), network.name.clone()),
        };
        Some(header::HeaderFacts {
            title,
            network: context,
            topic,
        })
    }

    /// The transcript and the typing line under it.
    pub fn render_transcript(&self, cx: &App) -> AnyElement {
        let tokens = Tokens::current(cx);
        let me = self.me.clone();
        let hub = self.hub.downgrade();
        let typing = self
            .hub
            .read(cx)
            .state
            .buffer(&self.buffer)
            .map(|held| transcript::typing_line(held, &tokens));
        v_flex()
            .id("irc-transcript-pane")
            .test_support()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(div().flex_1().min_h_0().child(transcript::scroller(
                self.scroll.state.clone(),
                &tokens,
                move |index, _, cx| {
                    let (row, issues) = me
                        .read_with(cx, |this, cx| this.row_at(index, cx))
                        .unwrap_or_else(|_| (div().into_any_element(), Vec::new()));
                    let (uploads, issues): (Vec<_>, Vec<_>) = issues.into_iter().partition(|key| {
                        subspace_api_client::chat_uploads::canonical(key).is_some()
                    });
                    let (prs, issues): (Vec<_>, Vec<_>) = issues
                        .into_iter()
                        .partition(|key| PullRequestLink::from_key(key).is_some());
                    _ = hub.update(cx, |hub, cx| {
                        crate::issues::want(hub, issues, cx);
                        crate::pull_requests::want(hub, prs, cx);
                        crate::uploads::cache::want(hub, uploads, cx);
                    });
                    row
                },
            )))
            .children(typing)
            .into_any_element()
    }
}

/// Who and which issues the buffer's messages may mention.
fn mentions(this: &Conversation, cx: &App) -> Mentions {
    let hub = this.hub.read(cx);
    let nicks = hub
        .state
        .buffer(&this.buffer)
        .map(|held| Mentions::new(held.members.iter().map(|member| member.nick.as_str())))
        .unwrap_or_default();
    nicks.with_issues(hub.issues.keys().clone())
}

/// Reference clicks become the conversation's [`ConversationEvent::Open`].
fn claim(conversation: &Conversation, cx: &App) -> Claim {
    let me = conversation.me.clone();
    let scope = conversation.hub.read(cx).issues.scope().cloned();
    Arc::new(move |found: &Ref, _, cx| {
        let found = found.clone();
        _ = me.update(cx, |conversation, cx| {
            let current_scope = conversation.hub.read(cx).issues.scope();
            if !matches!(found, Ref::Issue(_)) || current_scope == scope.as_ref() {
                cx.emit(ConversationEvent::Open(found));
            }
        });
    })
}

impl Render for Conversation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.upload_viewer.is_some() {
            let content = crate::uploads::viewer::render(self, cx);
            return find::scope(self, content, cx);
        }
        let tokens = Tokens::current(cx);
        let header = self.standalone_title(cx).map(|title| {
            h_flex()
                .flex_shrink_0()
                .min_h(tokens.toolbar_height)
                .px_3()
                .gap_2()
                .border_b_1()
                .border_color(tokens.line)
                .child(title)
        });
        let transcript = keys::transcript(self, cx)
            .flex_1()
            .min_h_0()
            .child(self.render_transcript(cx));
        let content = v_flex()
            .id("irc-conversation")
            .track_focus(&self.focus)
            .key_context(keys::CONVERSATION)
            .on_drop(cx.listener(|this, paths: &gpui_kit::ExternalPaths, _, cx| {
                crate::uploads::outgoing::paths(this, paths.0.to_vec(), cx);
            }))
            .on_action(cx.listener(|this, _: &keys::FocusTranscript, window, cx| {
                keys::enter(this, window, cx);
            }))
            .size_full()
            .min_h_0()
            .bg(tokens.surface)
            .children(header)
            .child(transcript)
            .child(div().flex_shrink_0().child(self.draft.render(
                &self.tray,
                !matches!(self.buffer.target, Target::Server),
                cx,
            )))
            .into_any_element();
        find::scope(self, content, cx)
    }
}
