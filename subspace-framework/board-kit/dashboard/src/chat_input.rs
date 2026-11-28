//! The chat input: a GPUI Kit [`InputGroup`] around a [`Textarea`] with
//! `submit_on_enter`, so Enter sends and Shift+Enter inserts a new line.
//! Kit reports Shift+Enter as a `PressEnter` too, after the line break, so
//! only a plain Enter sends. Option+Enter inserts a new line too, through
//! the `alt-enter` binding in this component's key context.
//!
//! The panel keeps the input in an `Entity<ChatInput>`, renders it and
//! subscribes to [`ChatInputEvent`]. The toolbar is passed in with
//! [`ChatInput::set_addon`], so this component knows nothing about it. The
//! shared comment [`crate::composer::Composer`] is a different interface and
//! stays unchanged.

use gpui_kit::component::input::{InputEvent, RopeExt as _, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _,
    IntoElement, KeyBinding, KeyContext, OperatingSystem, ParentElement as _, Render, SharedString,
    Styled as _, Subscription, Window, actions, div,
};

use crate::tokens::Tokens;
use attachments::IngestionBatch;
use std::fmt;
pub mod attachments;
pub mod completion;
mod counter;
mod message;
pub use counter::DraftLimits;
#[cfg(test)]
mod layout_tests;
mod resize;
mod sizing;
mod view;

actions!(
    subspace_chat_input,
    [
        ChatNewline,
        ChatBlockedSubmit,
        ChatNext,
        ChatPrevious,
        ChatComplete,
        ChatCompleteWithEnter,
        ChatCancel,
        ChatNewTab,
        ChatCloseTab,
        ChatNextTab,
        ChatPreviousTab,
        ChatRenameCancel,
        ChatSessionNext,
        ChatSessionPrevious,
        ChatSessionOpen,
        ChatSessionRename,
        ChatSessionDelete
    ]
);

pub const CHAT_INPUT_CONTEXT: &str = "SubspaceChatInput";
/// The chat panel's own context, which carries the tab keys.
pub const CHAT_PANEL_CONTEXT: &str = "SubspaceChat";
/// The rename editor's context, so Escape cancels only while it is open.
pub const CHAT_RENAME_CONTEXT: &str = "SubspaceChatRename";
/// The session list's context, which carries its own keys.
pub const CHAT_SESSIONS_CONTEXT: &str = "SubspaceChatSessions";
/// Added to the key context only while a completion list is open, so the
/// arrow keys keep moving the cursor the rest of the time.
pub const CHAT_SLASH_CONTEXT: &str = "SubspaceChatSlash";
const CHAT_BLOCKED_SUBMIT_CONTEXT: &str = "SubspaceChatBlockedSubmit";
/// Added to the key context of an input whose owner recalls sent lines with
/// Up and Down and completes with Tab, such as an IRC composer.
pub const CHAT_RECALL_CONTEXT: &str = "SubspaceChatRecall";
/// Added beside [`CHAT_RECALL_CONTEXT`] for inputs that handle Up and Down
/// as draft boundaries, message recall and completion navigation.
pub const CHAT_LINE_RECALL_CONTEXT: &str = "SubspaceChatLineRecall";
const MIN_ROWS: usize = 2;
const MAX_ROWS: usize = 12;

/// Binds Option+Enter ahead of the text area's own Enter binding, the
/// completion-list keys, which match only while the list is open, and the
/// tab keys of the chat panel. The shell calls this once, beside the other
/// key bindings.
pub fn bind_keys(cx: &mut App) {
    resize::bind_keys(cx);
    let input = Some("SubspaceChatInput > Input");
    let slash = Some("SubspaceChatSlash > Input");
    let blocked_submit = Some("SubspaceChatBlockedSubmit > Input");
    let panel = Some(CHAT_PANEL_CONTEXT);
    let rename = Some("SubspaceChatRename > Input");
    let sessions = Some(CHAT_SESSIONS_CONTEXT);
    let recall = Some("SubspaceChatRecall > Input");
    let line_recall = Some("SubspaceChatLineRecall > Input");
    cx.bind_keys([
        KeyBinding::new("up", ChatPrevious, line_recall),
        KeyBinding::new("down", ChatNext, line_recall),
        KeyBinding::new("tab", ChatComplete, recall),
        KeyBinding::new("alt-enter", ChatNewline, input),
        KeyBinding::new("down", ChatNext, slash),
        KeyBinding::new("up", ChatPrevious, slash),
        KeyBinding::new("tab", ChatComplete, slash),
        // Ahead of the text area's own Enter, which would also type a line
        // break where the platform delivers one as text.
        KeyBinding::new("enter", ChatCompleteWithEnter, slash),
        KeyBinding::new("enter", ChatBlockedSubmit, blocked_submit),
        KeyBinding::new("escape", ChatCancel, slash),
        KeyBinding::new("escape", ChatRenameCancel, rename),
        KeyBinding::new(crate::secondary!("t"), ChatNewTab, panel),
        KeyBinding::new(crate::secondary!("w"), ChatCloseTab, panel),
        // macOS reports shifted punctuation as the resulting character.
        KeyBinding::new(crate::secondary!("shift-]"), ChatNextTab, panel),
        KeyBinding::new(crate::secondary!("}"), ChatNextTab, panel),
        KeyBinding::new(crate::secondary!("shift-["), ChatPreviousTab, panel),
        KeyBinding::new(crate::secondary!("{"), ChatPreviousTab, panel),
        KeyBinding::new("down", ChatSessionNext, sessions),
        KeyBinding::new("up", ChatSessionPrevious, sessions),
        KeyBinding::new("enter", ChatSessionOpen, sessions),
        KeyBinding::new("f2", ChatSessionRename, sessions),
        KeyBinding::new("delete", ChatSessionDelete, sessions),
        KeyBinding::new("backspace", ChatSessionDelete, sessions),
    ]);
}

#[derive(Clone)]
pub struct AttachmentPaste(pub(crate) IngestionBatch);

impl fmt::Debug for AttachmentPaste {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl AttachmentPaste {
    pub fn batch(&self) -> &IngestionBatch {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub enum ChatInputEvent {
    /// Enter with text to send: already trimmed. Empty only when the owner
    /// opted in with [`ChatInput::set_submit_empty`].
    Submit(String),
    /// Enter or Tab while a completion list is open: the list completes.
    Complete,
    /// Arrow down or up while the list is open.
    Next,
    Previous,
    /// Escape while the list is open.
    Cancel,
    /// The text changed, for the `/` list.
    Changed(String),
    /// Non-text clipboard entries intercepted by the actual textarea paste action.
    PasteAttachments(AttachmentPaste),
}

/// The key behind the latest [`ChatInputEvent::Complete`], for an owner
/// whose list has a row only Enter may choose, such as one that runs the
/// draft ([`ChatInput::completion_key`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKey {
    Tab,
    Enter,
}

/// The text Enter sends, if it sends anything. Empty text sends nothing. A
/// running turn is no longer a reason to hold the text back: the message
/// steers that turn.
pub fn submission(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

// qual:allow(srp, god_struct) reason: "ChatInput is the shared composer facade; each setter is an owner option on one native textarea, and set_submit_empty is one more such option."
pub struct ChatInput {
    state: Entity<TextareaState>,
    attachment_budget: attachments::AttachmentBudget,
    addon: Option<AnyView>,
    /// A completion list is open, so Enter completes instead of sending.
    completing: bool,
    /// The chat shown takes no message, so the text area is disabled.
    disabled: bool,
    /// Up, Down and Tab go to the owner as Previous, Next and Complete.
    recall: bool,
    /// The draft is a line the owner recalled, still unedited, so Up and
    /// Down keep recalling even when it has several lines.
    recalled: bool,
    message: Option<message::MessageState>,
    limits: Option<DraftLimits>,
    row_limits: (usize, usize),
    /// Enter with an empty draft still submits, for an owner that sends
    /// something else with the text, such as attachments. Off by default.
    submit_empty: bool,
    /// The key behind the latest Complete.
    completion_key: Option<CompletionKey>,
    _subscription: Subscription,
}

impl EventEmitter<ChatInputEvent> for ChatInput {}

impl ChatInput {
    /// A composer that asks `engine`, the chat engine's display name.
    pub fn new(engine: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_placeholder(format!("Ask {engine}, / for commands"), window, cx)
    }

    fn with_placeholder(placeholder: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(placeholder)
                .submit_on_enter(true)
                .auto_grow(MIN_ROWS, MAX_ROWS)
        });
        let subscription = cx.subscribe(&state, Self::on_input_event);
        Self {
            state,
            attachment_budget: attachments::AttachmentBudget::empty_draft(),
            addon: None,
            completing: false,
            disabled: false,
            recall: false,
            recalled: false,
            message: None,
            limits: None,
            row_limits: (MIN_ROWS, MAX_ROWS),
            submit_empty: false,
            completion_key: None,
            _subscription: subscription,
        }
    }

    /// Whether Enter on an empty draft submits an empty text. Off by
    /// default; an owner turns it on while it has something else to send.
    // qual:api - Apps use this to submit attachment-only drafts.
    pub fn set_submit_empty(&mut self, submit_empty: bool, cx: &mut Context<Self>) {
        if self.submit_empty != submit_empty {
            self.submit_empty = submit_empty;
            cx.notify();
        }
    }

    /// The toolbar rendered in the group's bottom addon, or the leading tools
    /// on a compact message composer.
    pub fn set_addon(&mut self, addon: Option<AnyView>, cx: &mut Context<Self>) {
        self.addon = addon;
        cx.notify();
    }

    pub fn text(&self, cx: &App) -> String {
        self.state.read(cx).value().to_string()
    }

    /// Opts into the in-frame counter; the host owns protocol validation.
    /// Other hosts leave this unset and show no counter.
    pub fn set_limits(&mut self, limits: Option<DraftLimits>, cx: &mut Context<Self>) {
        if self.limits != limits {
            self.limits = limits;
            cx.notify();
        }
    }

    fn counter(&self, cx: &App) -> Option<counter::Counter> {
        self.limits?.counter(&self.text(cx))
    }

    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.to_owned();
        self.state
            .update(cx, |state, cx| state.set_value(text, window, cx));
        cx.notify();
    }

    /// The hint shown while the draft is empty, such as `Message #channel`.
    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = placeholder.into();
        self.state.update(cx, |state, cx| {
            state.set_placeholder(placeholder, window, cx)
        });
    }

    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_text("", window, cx);
    }

    /// The text cursor's byte offset, for completing the word before it.
    pub fn cursor(&self, cx: &App) -> usize {
        self.state.read(cx).cursor()
    }

    /// The selected byte range, or an empty range at the cursor.
    // qual:api - Formatting controls replace selected text.
    pub fn selection(&self, cx: &App) -> std::ops::Range<usize> {
        self.state.read(cx).selected_range()
    }

    /// Moves the selection to a byte range in the current draft.
    // qual:api - An inserted formatting pair can place its caret inside.
    pub fn set_selection(&mut self, range: std::ops::Range<usize>, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.set_selected_range(range, cx));
    }

    /// Replaces the bytes in `range` with `text`, leaving the cursor after
    /// it, as an edit the user can undo. The text then reports
    /// [`ChatInputEvent::Changed`].
    pub fn replace_range(
        &mut self,
        range: std::ops::Range<usize>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = text.to_owned();
        self.state.update(cx, |state, cx| {
            state.set_selected_range(range, cx);
            state.replace(text, window, cx);
        });
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |state, cx| state.focus(window, cx));
    }

    /// The key, Tab or Enter, behind the latest [`ChatInputEvent::Complete`]
    /// this input emitted; `None` before any. An owner reads it while
    /// handling that event.
    // qual:api
    pub fn completion_key(&self) -> Option<CompletionKey> {
        self.completion_key
    }

    /// Emits [`ChatInputEvent::Complete`] for `key`.
    fn complete(&mut self, key: CompletionKey, cx: &mut Context<Self>) {
        self.completion_key = Some(key);
        cx.emit(ChatInputEvent::Complete);
    }

    /// While a completion list is open, Enter emits
    /// [`ChatInputEvent::Complete`] rather than sending.
    pub fn set_completing(&mut self, completing: bool, cx: &mut Context<Self>) {
        self.completing = completing;
        cx.notify();
    }

    /// Up and Down emit [`ChatInputEvent::Previous`] and
    /// [`ChatInputEvent::Next`], and Tab emits [`ChatInputEvent::Complete`],
    /// for an owner that recalls sent lines and completes names. The arrow
    /// keys recall while the draft is empty or recalled. In an edited draft
    /// they jump to the start and end. When a completion list is open the
    /// arrows continue to move through its choices.
    // qual:api
    pub fn set_recall_keys(&mut self, recall: bool, cx: &mut Context<Self>) {
        self.recall = recall;
        cx.notify();
    }

    /// Whether the draft is a line the owner recalled and the user has not
    /// edited. Up and Down then keep recalling even when it has several
    /// lines, so a multi-line message does not stop the walk through the
    /// sent lines.
    // qual:api
    pub fn set_recalled(&mut self, recalled: bool, cx: &mut Context<Self>) {
        if self.recalled != recalled {
            self.recalled = recalled;
            cx.notify();
        }
    }

    /// Disables the text area, for a chat that takes no message, such as a
    /// replay of a cloud agent session.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        cx.notify();
    }

    fn on_input_event(
        &mut self,
        _: Entity<TextareaState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                if let Some(message) = &mut self.message {
                    message.error = None;
                }
                let text = self.text(cx);
                cx.emit(ChatInputEvent::Changed(text));
                cx.notify();
            }
            InputEvent::PressEnter { shift: false, .. } => self.enter(cx),
            _ => {}
        }
    }

    fn enter(&mut self, cx: &mut Context<Self>) {
        if self.disabled
            || self
                .message
                .as_ref()
                .is_some_and(|message| message.submit_blocked)
        {
            return;
        }
        if self.completing {
            self.complete(CompletionKey::Enter, cx);
            return;
        }
        match submission(&self.text(cx)) {
            Some(text) => cx.emit(ChatInputEvent::Submit(text)),
            None if self.submit_empty => cx.emit(ChatInputEvent::Submit(String::new())),
            None => {}
        }
    }

    fn newline(&mut self, _: &ChatNewline, window: &mut Window, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.insert("\n", window, cx));
    }

    /// Completion bindings match only while a list is open.
    fn key_context(&self, os: OperatingSystem) -> KeyContext {
        let mut context = KeyContext::new_with_defaults(os);
        context.add(CHAT_INPUT_CONTEXT);
        if self
            .message
            .as_ref()
            .is_some_and(|message| message.submit_blocked)
        {
            context.add(CHAT_BLOCKED_SUBMIT_CONTEXT);
        } else if self.completing {
            context.add(CHAT_SLASH_CONTEXT);
        }
        if self.recall {
            context.add(CHAT_RECALL_CONTEXT);
            context.add(CHAT_LINE_RECALL_CONTEXT);
        }
        context
    }

    fn vertical_arrow(&mut self, previous: bool, window: &mut Window, cx: &mut Context<Self>) {
        let end = self.state.read(cx).text().len();
        if self.completing || self.recalled || end == 0 {
            let event = if previous {
                ChatInputEvent::Previous
            } else {
                ChatInputEvent::Next
            };
            cx.emit(event);
        } else {
            let offset = if previous { 0 } else { end };
            self.state.update(cx, |state, cx| {
                let position = state.text().offset_to_position(offset);
                state.set_cursor_position(position, window, cx);
            });
        }
    }
}

impl Render for ChatInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        resize::sync_rows(self, window, cx);
        let tokens = Tokens::current(cx);
        div()
            .id("chat-input-frame")
            .relative()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .when(self.compact_label().is_none(), |frame| {
                frame.border_t_1().border_color(tokens.line)
            })
            .key_context(self.key_context(cx.platform_info().os))
            .on_action(cx.listener(Self::newline))
            .on_action(|_: &ChatBlockedSubmit, _, _| {})
            .on_action(
                cx.listener(|this, _: &ChatNext, window, cx| {
                    this.vertical_arrow(false, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &ChatPrevious, window, cx| {
                this.vertical_arrow(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ChatComplete, _, cx| {
                this.complete(CompletionKey::Tab, cx);
            }))
            .on_action(cx.listener(|this, _: &ChatCompleteWithEnter, _, cx| {
                this.complete(CompletionKey::Enter, cx);
            }))
            .on_action(cx.listener(|_, _: &ChatCancel, _, cx| cx.emit(ChatInputEvent::Cancel)))
            .text_size(tokens.row_font)
            .on_drag_move::<resize::InputResize>(cx.listener(resize::drag))
            .child(view::group(self, window, cx))
            .children(resize::handle(self, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{ChatInput, ChatInputEvent, submission};
    use crate::chat_input::attachments::{AttachmentSource, MAX_DRAFT_IMAGES};
    use gpui_kit::component::input::Paste;
    use gpui_kit::{
        AppContext as _, ClipboardEntry, ClipboardItem, Entity, Image, ImageFormat, IntoElement,
        ParentElement as _, Render, Subscription, TestAppContext, Window, div,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use subspace_agent_model::DRAFT_ATTACHMENT_LIMIT;

    struct PasteHarness {
        input: Entity<ChatInput>,
        _subscription: Subscription,
    }

    impl PasteHarness {
        fn new(
            events: Rc<RefCell<Vec<ChatInputEvent>>>,
            window: &mut Window,
            cx: &mut gpui_kit::Context<Self>,
        ) -> Self {
            let input = cx.new(|cx| ChatInput::new("Copilot", window, cx));
            let subscription = cx.subscribe(&input, move |_, _, event, _| {
                events.borrow_mut().push(event.clone());
            });
            Self {
                input,
                _subscription: subscription,
            }
        }
    }

    impl Render for PasteHarness {
        fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
            div().child(self.input.clone())
        }
    }

    #[test]
    fn enter_sends_trimmed_text_and_nothing_else() {
        assert_eq!(submission("  hello \n").as_deref(), Some("hello"));
        assert_eq!(submission("   "), None, "empty text");
        assert_eq!(
            submission("steer left").as_deref(),
            Some("steer left"),
            "a running turn no longer blocks the input"
        );
    }

    #[gpui_kit::test]
    fn empty_enter_submits_only_when_the_owner_opts_in(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let (harness, cx) =
            cx.add_window_view(|window, cx| PasteHarness::new(captured, window, cx));
        let input = harness.read_with(cx, |harness, _| harness.input.clone());
        input.update(cx, |input, cx| input.enter(cx));
        assert!(events.borrow().is_empty(), "off by default");
        input.update(cx, |input, cx| {
            input.set_submit_empty(true, cx);
            input.enter(cx);
        });
        assert!(matches!(
            events.borrow().as_slice(),
            [ChatInputEvent::Submit(text)] if text.is_empty()
        ));
    }

    #[gpui_kit::test]
    fn textarea_paste_emits_images_but_preserves_ordinary_text(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let (harness, cx) =
            cx.add_window_view(|window, cx| PasteHarness::new(captured, window, cx));
        cx.update(|window, cx| {
            let input = harness.read(cx).input.clone();
            input.update(cx, |input, cx| input.focus(window, cx));
            let _ = window.draw(cx);
            cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
                ImageFormat::Png,
                vec![1, 2, 3],
            )));
            window.dispatch_action(Box::new(Paste), cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("ordinary text".into()));
            window.dispatch_action(Box::new(Paste), cx);
        });
        cx.run_until_parked();
        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                ChatInputEvent::PasteAttachments(super::AttachmentPaste(batch))
                    if matches!(batch.sources.as_slice(), [AttachmentSource::ClipboardImage { .. }])
            )),
            "the actual textarea callback should emit one bounded image batch"
        );
        assert_eq!(
            cx.update(|_, cx| harness.read(cx).input.read(cx).text(cx)),
            "ordinary text"
        );
    }

    #[gpui_kit::test]
    fn textarea_paste_never_emits_unbounded_image_payloads(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let (harness, cx) =
            cx.add_window_view(|window, cx| PasteHarness::new(captured, window, cx));
        cx.update(|window, cx| {
            let input = harness.read(cx).input.clone();
            input.update(cx, |input, cx| input.focus(window, cx));
            let _ = window.draw(cx);
            let entries =
                std::iter::once(ClipboardEntry::Image(Image::from_bytes(
                    ImageFormat::Png,
                    vec![0; crate::images::MAX_ENCODED_BYTES + 1],
                )))
                .chain((0..3).map(|_| {
                    ClipboardEntry::Image(Image::from_bytes(
                        ImageFormat::Png,
                        vec![1_u8; crate::images::MAX_ENCODED_BYTES],
                    ))
                }))
                .chain((0..MAX_DRAFT_IMAGES + 20).map(|_| {
                    ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, vec![1_u8; 1]))
                }))
                .collect();
            cx.write_to_clipboard(ClipboardItem { entries });
            window.dispatch_action(Box::new(Paste), cx);
        });
        cx.run_until_parked();
        let events = events.borrow();
        let Some(ChatInputEvent::PasteAttachments(super::AttachmentPaste(batch))) = events.last()
        else {
            panic!("the actual textarea callback did not emit attachments");
        };
        let image_bytes = batch
            .sources
            .iter()
            .filter_map(|source| match source {
                AttachmentSource::ClipboardImage { bytes, .. } => Some(bytes.len()),
                AttachmentSource::Path(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(image_bytes.len(), MAX_DRAFT_IMAGES);
        assert_eq!(
            image_bytes
                .iter()
                .filter(|size| **size == crate::images::MAX_ENCODED_BYTES)
                .count(),
            2
        );
        assert!(image_bytes.iter().sum::<usize>() <= DRAFT_ATTACHMENT_LIMIT);
        assert!(batch.notice.is_some());
    }
}
