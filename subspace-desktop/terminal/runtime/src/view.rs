//! The GPUI entity the board embeds, one per tab: it owns one session, the
//! render state that reads it and the style it draws with (REQ-12 to REQ-17).
//!
//! The session and the emulator are `!Send`, so they stay in the entity and
//! every call reaches them from the GPUI foreground: a task drains the
//! session's reader channel and asks for one repaint per drain, and a second
//! task flips the blink phase. The state itself lives in `crate::state`
//! behind an `Rc<RefCell<_>>`, which the element shares, because GPUI renders
//! a view inside that view's own entity update and an element may not update
//! or read the entity again.
//!
//! # What the board does
//!
//! ```ignore
//! let (style, warnings) = TerminalStyle::resolve(&config, &theme, cx);
//! let view = TerminalView::open(&options, style, cx)?;
//! // one tab per view; the board shows `view.read(cx).title()` and
//! // `view.read(cx).exit()`, and calls `apply_config` after a reload (task 11).
//! ```
//!
//! # Input
//!
//! The view's key context carries `subspace_framework::dashboard::RAW_KEYS_CONTEXT`, so
//! the shell's and the board's plain keys stand aside while a session has
//! focus and the keys reach the program instead (REQ-18 to REQ-21). The
//! focusable `div` [`Render`] returns owns the key listener (`view::keys`),
//! the copy and paste actions and the session's context menu
//! (`view::clipboard`), and the element registers the platform input handler
//! and the pointer listeners against its own hitbox (`crate::element`).
//! A board binds copy and paste once with [`crate::bind_keys`].

// qual:allow(coupling, max_instability=0.91) reason: "The view is the crate's one entity: it holds the state, embeds the element, owns the key, input-method and clipboard wiring and exposes the session to the board, and only the crate root depends on it. Every dependency points at a leaf module, which is the direction the stable-dependencies principle asks for."

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, KeyContext, OperatingSystem, ParentElement as _, Render,
    SharedString, Stateful, Styled, Subscription, Task, Window, div,
};
use subspace_framework::dashboard::anchored::AnchoredContextMenuExt as _;
use subspace_framework::dashboard::{RAW_KEYS_CONTEXT, Tokens};

use crate::clipboard::{CopySelection, PasteClipboard};
use crate::command::SessionOptions;
use crate::element::TerminalElement;
use crate::error::SessionError;
use crate::exit::SessionExit;
use crate::pointer::Pointer;
use crate::session::Session;
use crate::state::{BLINK_INTERVAL, DRAIN_INTERVAL, TerminalState};
use crate::style::TerminalStyle;
use crate::typing;

pub(crate) mod clipboard;
mod keys;

/// The key context of a focused session.
pub const TERMINAL_CONTEXT: &str = "SubspaceTerminal";

/// One session, drawn.
pub struct TerminalView {
    state: Rc<RefCell<TerminalState>>,
    /// The pointer state the element's listeners drive (REQ-23).
    pub(crate) pointer: Rc<RefCell<Pointer>>,
    /// The paste the user has been asked about, while the confirmation is
    /// open (REQ-22).
    pub(crate) pending_paste: Option<String>,
    focus: FocusHandle,
    _drain: Task<()>,
    _blink: Task<()>,
    /// Repaints the session when the window is activated or deactivated, so
    /// the cursor turns hollow with the window (REQ-15). Registered on the
    /// first render, which is where the view first sees a window.
    activation: Option<Subscription>,
}

impl TerminalView {
    /// Start a session and the view that draws it.
    ///
    /// The session starts on a default grid; the element measures the real
    /// one during its first layout and resizes the session then (REQ-17).
    pub fn open(
        options: &SessionOptions,
        style: TerminalStyle,
        cx: &mut App,
    ) -> Result<Entity<Self>, SessionError> {
        let state = TerminalState::start(options, style)?;
        let pointer = Pointer::new()?;
        Ok(cx.new(|cx| Self::new(state, pointer, cx)))
    }

    /// Assemble the view around a started session and start its two
    /// foreground tasks.
    pub(crate) fn new(
        state: TerminalState,
        pointer: Rc<RefCell<Pointer>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = Rc::new(RefCell::new(state));
        state.borrow_mut().apply_style();
        Self {
            state,
            pointer,
            pending_paste: None,
            focus: cx.focus_handle(),
            _drain: drain_task(cx),
            _blink: blink_task(cx),
            activation: None,
        }
    }

    /// The session the view draws, for the board's tab model.
    pub fn session(&self) -> SessionRef<'_> {
        SessionRef(self.state.borrow())
    }

    /// The session the view draws, writable, which a board uses to send a
    /// program input of its own.
    // qual:api (the key, mouse and paste encoders write through the state;
    // this is the board's own way in)
    pub fn session_mut(&self) -> SessionMut<'_> {
        SessionMut(self.state.borrow_mut())
    }

    /// The title the program set with OSC 0 or OSC 2, and `None` until it
    /// sets one, when the tab shows the program name instead (REQ-07).
    pub fn title(&self) -> Option<SharedString> {
        self.state.borrow().title.clone()
    }

    /// How the program ended, once it has (REQ-09).
    // qual:api (the board offers Restart on an ended session)
    pub fn exit(&self) -> Option<SessionExit> {
        self.state.borrow().session.exit().cloned()
    }

    /// The last failure the view recorded, which the board shows in its
    /// status beside the configuration warnings (REQ-32).
    // qual:api (the board shows this)
    pub fn failure(&self) -> Option<String> {
        self.state
            .borrow()
            .failure
            .as_ref()
            .map(SessionError::to_string)
    }

    /// Draw with a new style: the colours, the fonts, the cursor defaults,
    /// the padding of a reload or an appearance change (REQ-12, REQ-14,
    /// REQ-33). The next layout re-measures the cell and resizes the session.
    // qual:api (task 11 applies a reload through this)
    pub fn apply_config(&self, style: TerminalStyle, cx: &mut Context<Self>) {
        crate::config::apply_config(&mut self.state.borrow_mut(), style);
        cx.notify();
    }

    /// Scroll by whole rows, up for a negative count (REQ-16).
    // qual:api (the board scrolls a session through this; the view's own
    // Shift+Page and scrollbar drag use the three methods below)
    pub fn scroll_rows(&self, rows: isize, cx: &mut Context<Self>) {
        self.state.borrow_mut().scroll_rows(rows);
        cx.notify();
    }

    /// Scroll by pages, up for a negative count, for Shift+Page (REQ-16).
    pub fn scroll_pages(&self, pages: f32, cx: &mut Context<Self>) {
        self.state.borrow_mut().scroll_pages(pages);
        cx.notify();
    }

    /// Scroll to a position on the scrollbar track, as a fraction from its
    /// top, which is what a scrollbar drag produces (REQ-16).
    pub fn scroll_to_fraction(&self, fraction: f32, cx: &mut Context<Self>) {
        self.state.borrow_mut().scroll_to_fraction(fraction);
        cx.notify();
    }

    /// Follow new output again (REQ-16).
    pub fn scroll_to_bottom(&self, cx: &mut Context<Self>) {
        self.state.borrow_mut().scroll_to_bottom();
        cx.notify();
    }

    /// The state the element draws from.
    pub(crate) fn state(&self) -> Rc<RefCell<TerminalState>> {
        Rc::clone(&self.state)
    }

    /// Apply the session's output, exchange what the program and GPUI's
    /// clipboard owe each other, and repaint once (REQ-09, REQ-24).
    fn drain(&mut self, cx: &mut Context<Self>) {
        let changed = self.state.borrow_mut().drain();
        clipboard::exchange(self, cx);
        if changed {
            cx.notify();
        }
    }

    /// Show the other half of the blink cycle (REQ-13, REQ-15).
    fn flip_blink(&mut self, cx: &mut Context<Self>) {
        if self.state.borrow_mut().flip_blink() {
            cx.notify();
        }
    }

    /// Repaint when the window is activated or deactivated, which is how the
    /// cursor learns to turn hollow with the window (REQ-15). The view first
    /// sees a window in `render`, so the observer is registered there, once.
    fn watch_activation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activation.is_some() {
            return;
        }
        self.activation = Some(cx.observe_window_activation(window, |_view, _window, cx| {
            cx.notify();
        }));
    }
}

/// Whether the session draws as focused: its own focus handle has the focus
/// and its window is the active one. Ghostty hollows the cursor when its
/// window is not active, and REQ-15 asks for the same.
fn showing_focus(focus: &FocusHandle, window: &Window) -> bool {
    focus.is_focused(window) && window.is_window_active()
}

/// A borrowed view of the session, for the facts a board reads.
pub struct SessionRef<'a>(std::cell::Ref<'a, TerminalState>);

impl std::ops::Deref for SessionRef<'_> {
    type Target = Session;

    fn deref(&self) -> &Session {
        &self.0.session
    }
}

/// A borrowed, writable view of the session, for the input encoders.
pub struct SessionMut<'a>(std::cell::RefMut<'a, TerminalState>);

impl std::ops::Deref for SessionMut<'_> {
    type Target = Session;

    fn deref(&self) -> &Session {
        &self.0.session
    }
}

impl std::ops::DerefMut for SessionMut<'_> {
    fn deref_mut(&mut self) -> &mut Session {
        &mut self.0.session
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.watch_activation(window, cx);
        self.state.borrow_mut().focused = showing_focus(&self.focus, window);
        let element = TerminalElement::new(
            self.state(),
            Rc::clone(&self.pointer),
            cx.entity(),
            self.focus.clone(),
        );
        let offers_menu = clipboard::offers_menu(self);
        let session = div()
            .id("subspace-terminal-session")
            .key_context(key_context(cx.platform_info().os))
            .track_focus(&self.focus)
            .size_full()
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(|view, _: &CopySelection, _window, cx| {
                clipboard::copy(view, cx);
            }))
            .on_action(cx.listener(|view, _: &PasteClipboard, _window, cx| {
                clipboard::paste(view, cx);
            }))
            .child(element)
            .children(self.pre_edit(cx))
            .children(self.paste_confirmation(cx));
        with_menu(session, offers_menu)
    }
}

/// The session, with its own context menu while the program is not asking
/// for the pointer: a reporting program receives the right click instead
/// (REQ-22, REQ-23).
fn with_menu(session: Stateful<Div>, offers: bool) -> AnyElement {
    if offers {
        return session
            .anchored_context_menu(|menu, _window, _cx| clipboard::menu(menu))
            .into_any_element();
    }
    session.into_any_element()
}

impl TerminalView {
    /// The confirmation a refused paste shows, while one is waiting
    /// (REQ-22).
    fn paste_confirmation(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.pending_paste
            .is_some()
            .then(|| clipboard::confirmation(self, cx))
    }

    /// The marked text of a composition in progress, drawn over the cursor
    /// cell it is being typed at (REQ-18).
    fn pre_edit(&self, cx: &App) -> Option<AnyElement> {
        let (text, at) = typing::composition(&self.state.borrow())?;
        let tokens = Tokens::current(cx);
        Some(
            div()
                .absolute()
                .left(at.offset.x)
                .top(at.offset.y)
                .h(at.rect.size.height)
                .bg(tokens.pane)
                .text_color(tokens.fg_strong)
                .border_b_1()
                .border_color(tokens.accent)
                .child(SharedString::from(text))
                .into_any_element(),
        )
    }
}

/// The view's key context.
///
/// `RAW_KEYS_CONTEXT` is what keeps the shell's F-key and, on Linux, `ctrl`
/// bindings from acting while a session has focus, so those keys reach the
/// program instead (REQ-18, REQ-21). Task 08 binds its own keys against the
/// same context.
fn key_context(os: OperatingSystem) -> KeyContext {
    let mut context = KeyContext::new_with_defaults(os);
    context.add(RAW_KEYS_CONTEXT);
    context.add(TERMINAL_CONTEXT);
    context
}

/// Apply the session's output once per interval, for as long as the view
/// lives.
fn drain_task(cx: &mut Context<TerminalView>) -> Task<()> {
    cx.spawn(async move |view, cx| {
        loop {
            cx.background_executor().timer(DRAIN_INTERVAL).await;
            if view.update(cx, TerminalView::drain).is_err() {
                return;
            }
        }
    })
}

/// Flip the blink phase for as long as the view lives.
fn blink_task(cx: &mut Context<TerminalView>) -> Task<()> {
    cx.spawn(async move |view, cx| {
        loop {
            cx.background_executor().timer(BLINK_INTERVAL).await;
            if view.update(cx, TerminalView::flip_blink).is_err() {
                return;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::view::key_context;
    use gpui_kit::OperatingSystem;

    #[test]
    fn the_key_context_owns_the_raw_keys_so_shell_keys_stand_aside() {
        let context = key_context(OperatingSystem::Mac);

        assert!(context.contains("SubspaceRawKeys"));
        assert!(context.contains("SubspaceTerminal"));
    }
}
