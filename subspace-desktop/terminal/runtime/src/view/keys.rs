//! The view's key listener and its platform input handler (REQ-18, REQ-19,
//! REQ-31a).
//!
//! Two paths reach a session. GPUI dispatches action bindings first, so the
//! shell's and the board's own keys act before anything here; what is left
//! arrives as a `KeyDownEvent` and goes to the encoder through
//! [`crate::input`]. Platform input methods take the second path: the window
//! holds an [`EntityInputHandler`] for the focused session, marked text stays
//! in the view until the method commits it, and the cursor cell rectangle
//! tells the method where to put its candidate window.

use std::ops::Range;

use gpui_kit::{
    Bounds, Context, EntityInputHandler, KeyDownEvent, Keystroke, Pixels, Point, UTF16Selection,
    Window,
};

use crate::typing;
use crate::view::TerminalView;

/// How far Shift+Page moves, in pages (REQ-16).
const PAGE: f32 = 1.0;

impl TerminalView {
    /// Send one key press to the program, or scroll where the key is one of
    /// the two the board keeps for itself (REQ-16, REQ-18).
    pub(crate) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pages) = scrolled_pages(&event.keystroke) {
            self.scroll_pages(pages, cx);
            cx.stop_propagation();
            return;
        }
        if typing::type_keystroke(&mut self.state.borrow_mut(), &event.keystroke) {
            cx.stop_propagation();
            cx.notify();
        }
    }
}

/// The pages Shift+Page scrolls, and nothing for every other key: those two
/// are the only keys a focused session does not send on (REQ-16).
fn scrolled_pages(keystroke: &Keystroke) -> Option<f32> {
    if !keystroke.modifiers.shift || keystroke.modifiers.control || keystroke.modifiers.alt {
        return None;
    }
    match keystroke.key.as_str() {
        "pageup" => Some(-PAGE),
        "pagedown" => Some(PAGE),
        _ => None,
    }
}

/// How many UTF-16 code units a string holds, which is what the platform
/// input-method ranges count in.
fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

impl EntityInputHandler for TerminalView {
    /// A session holds no editable document, so there is no text to report
    /// for a range: a program owns its screen and the view only forwards
    /// input.
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    /// The insertion point an input method composes at, which is the end of
    /// the marked text.
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = utf16_len(self.state.borrow().typing.pre_edit.text());
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    /// The marked text of a composition in progress, which is what tells the
    /// platform that the session is composing.
    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let state = self.state.borrow();
        let pre_edit = &state.typing.pre_edit;
        pre_edit.is_active().then(|| 0..utf16_len(pre_edit.text()))
    }

    /// Drop a composition the platform cancelled; the program never saw it.
    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.borrow_mut().typing.pre_edit.clear();
        cx.notify();
    }

    /// Send committed text to the program (REQ-18).
    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        typing::commit_text(&mut self.state.borrow_mut(), text);
        cx.notify();
    }

    /// Hold new marked text, which is drawn at the cursor until the method
    /// commits it (REQ-18).
    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.borrow_mut().typing.pre_edit.mark(new_text);
        cx.notify();
    }

    /// The cursor cell, so a candidate window opens beside the text being
    /// composed (REQ-18).
    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(typing::caret(&self.state.borrow()).rect)
    }

    /// A session holds no editable document, so a window position names no
    /// character index.
    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Keystroke, Modifiers};

    use crate::view::keys::{scrolled_pages, utf16_len};

    /// A keystroke with the given key and modifiers.
    fn keystroke(key: &str, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_owned(),
            key_char: None,
        }
    }

    #[test]
    fn shift_and_the_page_keys_scroll_and_nothing_else_does() {
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };

        assert_eq!(scrolled_pages(&keystroke("pageup", shift)), Some(-1.0));
        assert_eq!(scrolled_pages(&keystroke("pagedown", shift)), Some(1.0));
        assert_eq!(scrolled_pages(&keystroke("up", shift)), None);
        assert_eq!(
            scrolled_pages(&keystroke("pageup", Modifiers::default())),
            None,
            "a plain page key is the program's"
        );
    }

    #[test]
    fn a_page_key_with_another_modifier_goes_to_the_program() {
        let with_control = Modifiers {
            shift: true,
            control: true,
            ..Modifiers::default()
        };
        let with_alt = Modifiers {
            shift: true,
            alt: true,
            ..Modifiers::default()
        };

        assert_eq!(scrolled_pages(&keystroke("pageup", with_control)), None);
        assert_eq!(scrolled_pages(&keystroke("pagedown", with_alt)), None);
    }

    #[test]
    fn a_composition_is_measured_in_utf16_code_units() {
        assert_eq!(utf16_len(""), 0);
        assert_eq!(utf16_len("にほ"), 2);
        assert_eq!(utf16_len("🙂"), 2);
    }
}
