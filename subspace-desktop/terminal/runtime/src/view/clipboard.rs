//! The session's copy and paste (REQ-22 to REQ-24).
//!
//! GPUI's clipboard is only reachable with an `App`, so everything that
//! touches it happens here, in the view's own handlers and once per drain:
//!
//! - **Copy** puts the session's selection on the system clipboard, and
//!   does nothing without one.
//! - **Paste** sends the clipboard, bracketed when the program asked for
//!   that mode, and asks first when `clipboard-paste-protection` applies.
//! - **Copy on select** puts a finished selection on the clipboards the
//!   Ghostty value names.
//! - A program's own **clipboard sequences** are answered through
//!   `crate::effects`: the clipboard is offered to it only while
//!   `clipboard-read` allows a read, the writes it asked for are applied
//!   after the drain that produced them, and the replies libghostty wrote
//!   for it are sent to the pseudo-terminal then too.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{
    AnyElement, App, ClipboardItem, Context, Div, ParentElement as _, SharedString, Styled as _,
    div,
};
use libghostty_vt::terminal::Mode;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::dialog::{self, DialogWidth};

use crate::clipboard::{self, CopySelection, CopyTarget, PasteClipboard};
use crate::pointer;
use crate::state::TerminalState;
use crate::view::TerminalView;

/// The title of the paste confirmation (REQ-22).
const CONFIRM_TITLE: &str = "Paste into the session?";

/// Why a paste is confirmed, which is the one case REQ-22 names.
const CONFIRM_REASON: &str =
    "This text holds a line break, so the program would run it as soon as it arrives.";

/// How much of a refused paste the confirmation shows, in characters, so a
/// long paste neither fills the window nor hides what it starts with.
const PREVIEW_CHARS: usize = 400;

/// Put the session's selection on the system clipboard, and do nothing
/// without a selection (REQ-22).
pub(crate) fn copy(view: &TerminalView, cx: &mut App) {
    if let Some(text) = pointer::selected_text(&view.state.borrow()) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

/// Send the system clipboard to the program, asking first when the paste
/// could run a command (REQ-22).
pub(crate) fn paste(view: &mut TerminalView, cx: &mut Context<TerminalView>) {
    let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
        return;
    };
    if confirms(view, &text) {
        view.pending_paste = Some(text);
    } else {
        send(view, &text);
    }
    cx.notify();
}

/// Send the paste the user confirmed (REQ-22).
pub(crate) fn send_pending(view: &mut TerminalView, cx: &mut Context<TerminalView>) {
    if let Some(text) = view.pending_paste.take() {
        send(view, &text);
    }
    cx.notify();
}

/// Drop the paste the user refused (REQ-22).
pub(crate) fn cancel_pending(view: &mut TerminalView, cx: &mut Context<TerminalView>) {
    view.pending_paste = None;
    cx.notify();
}

/// Whether a right click opens the session's own menu, which it does only
/// while the program is not asking for the pointer (REQ-23).
pub(crate) fn offers_menu(view: &TerminalView) -> bool {
    pointer::offers_menu(&view.state.borrow())
}

/// Carry everything between the session and GPUI's clipboard, once per
/// drain: what a program may read, what it asked to write, the replies it
/// is waiting for and what a finished selection copies (REQ-23, REQ-24).
pub(crate) fn exchange(view: &TerminalView, cx: &mut App) {
    offer(view, cx);
    apply_writes(view, cx);
    send_replies(view);
    copy_on_select(view, cx);
}

/// Whether this paste has to be confirmed first (REQ-22).
fn confirms(view: &TerminalView, text: &str) -> bool {
    let state = view.state.borrow();
    state
        .style
        .clipboard
        .confirms_paste(text, bracketed(&state))
}

/// Whether the program enabled bracketed paste (REQ-22).
fn bracketed(state: &TerminalState) -> bool {
    state
        .session
        .terminal
        .mode(Mode::BRACKETED_PASTE)
        .unwrap_or(false)
}

/// Write one paste to the program, bracketed when it asked for that mode.
fn send(view: &TerminalView, text: &str) {
    let mut state = view.state.borrow_mut();
    let encoded = clipboard::encode_paste(text, bracketed(&state));
    match encoded {
        Ok(bytes) => state.failure = state.session.write(&bytes).err(),
        Err(error) => state.failure = Some(error),
    }
}

/// Offer the clipboard to the program, and only while the configuration
/// allows it to read one (REQ-24).
fn offer(view: &TerminalView, cx: &App) {
    let effects = Rc::clone(&view.state.borrow().clipboard);
    let offers = effects.borrow().offers_reads();
    let text = offers
        .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
        .flatten();
    effects.borrow_mut().offer(text);
}

/// Put what the program wrote on the real clipboard (REQ-24).
fn apply_writes(view: &TerminalView, cx: &mut App) {
    let effects = Rc::clone(&view.state.borrow().clipboard);
    for write in effects.borrow_mut().take_writes() {
        let destination = match write.primary {
            true => CopyTarget::Primary,
            false => CopyTarget::Clipboard,
        };
        copy_to(&[destination], &write.text, cx);
    }
}

/// Send the program the replies libghostty wrote for it, which a clipboard
/// read and every other query produce (REQ-24).
fn send_replies(view: &TerminalView) {
    let mut state = view.state.borrow_mut();
    let replies = state.clipboard.borrow_mut().take_replies();
    if replies.is_empty() {
        return;
    }
    state.failure = state.session.write(&replies).err();
}

/// Copy a finished selection to the clipboards `copy-on-select` names
/// (REQ-23).
fn copy_on_select(view: &TerminalView, cx: &mut App) {
    let Some(text) = view.pointer.borrow_mut().take_copied() else {
        return;
    };
    let destinations = view.state.borrow().style.clipboard.copy_on_select();
    copy_to(destinations, &text, cx);
}

/// Put one text on the clipboards a copy names (REQ-22, REQ-23).
fn copy_to(destinations: &[CopyTarget], text: &str, cx: &mut App) {
    for destination in destinations {
        write_clipboard(*destination, text, cx);
    }
}

/// Put one text on one clipboard. A primary selection only exists on Linux,
/// so that destination never reaches this on macOS or Windows (REQ-23).
fn write_clipboard(destination: CopyTarget, text: &str, cx: &mut App) {
    let item = ClipboardItem::new_string(text.to_owned());
    match destination {
        CopyTarget::Clipboard => cx.write_to_clipboard(item),
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        CopyTarget::Primary => cx.write_to_primary(item),
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        CopyTarget::Primary => drop(item),
    }
}

/// The copy and paste entries of the session's context menu, from the
/// shared menu widget (REQ-22).
pub(crate) fn menu(menu: PopupMenu) -> PopupMenu {
    menu.menu("Copy", Box::new(CopySelection))
        .menu("Paste", Box::new(PasteClipboard))
}

/// The confirmation a refused paste shows, using the shared dialog
/// treatment (REQ-22).
pub(crate) fn confirmation(view: &TerminalView, cx: &mut Context<TerminalView>) -> AnyElement {
    let tokens = Tokens::current(cx);
    let preview = preview(view.pending_paste.as_deref().unwrap_or_default());
    let body = dialog::frame(&tokens, DialogWidth::D540, CONFIRM_TITLE)
        .child(div().text_color(tokens.fg).child(CONFIRM_REASON))
        .child(
            div()
                .p_2()
                .rounded_md()
                .bg(tokens.bg)
                .text_color(tokens.muted)
                .child(preview),
        )
        .child(buttons(cx));
    dialog::backdrop(&tokens, body)
}

/// The confirmation's two actions, in secondary-to-primary order.
fn buttons(cx: &mut Context<TerminalView>) -> Div {
    dialog::footer()
        .child(
            Button::new("cancel-paste")
                .label("Cancel")
                .on_click(cx.listener(|view, _, _window, cx| cancel_pending(view, cx))),
        )
        .child(
            Button::new("send-paste")
                .primary()
                .label("Paste")
                .on_click(cx.listener(|view, _, _window, cx| send_pending(view, cx))),
        )
}

/// The start of a refused paste, as the confirmation shows it.
fn preview(text: &str) -> SharedString {
    match text.char_indices().nth(PREVIEW_CHARS) {
        Some((end, _)) => SharedString::from(format!("{}…", &text[..end])),
        None => SharedString::from(text.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use crate::view::clipboard::{PREVIEW_CHARS, preview};

    #[test]
    fn a_long_paste_is_previewed_by_its_start() {
        let long = "x".repeat(PREVIEW_CHARS * 2);

        let shown = preview(&long);

        assert!(shown.ends_with('…'));
        assert_eq!(shown.chars().count(), PREVIEW_CHARS + 1);
    }

    #[test]
    fn a_short_paste_is_previewed_whole() {
        assert_eq!(preview("one\ntwo"), "one\ntwo");
        assert_eq!(preview(""), "");
    }
}
