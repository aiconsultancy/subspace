//! A saved credential's field. The input never shows the saved value: while
//! a secret is saved the input is read-only, Clear makes it editable again,
//! and text entered after Clear replaces the secret on Save. Used by the
//! shell's Settings pages and by board-owned settings editors.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::h_flex;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, Entity, InteractiveElement as _, IntoElement as _,
    MouseDownEvent, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div,
};
use subspace_lucide::IconName;

use crate::Tokens;
use crate::icon_button::IconButton;

/// The words a secret row uses for its kind of credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretWords {
    pub saved: &'static str,
    pub clear: &'static str,
    pub undo: &'static str,
    pub clear_to_change: &'static str,
    pub clear_on_save: &'static str,
}

/// An access token, such as a GitHub token.
pub const TOKEN: SecretWords = SecretWords {
    saved: "Token saved",
    clear: "Clear saved token",
    undo: "Undo clear",
    clear_to_change: "Clear the saved token to change it.",
    clear_on_save: "Save removes the saved token, or replaces it with the token you enter.",
};

/// A password, such as an IRC server or SASL password.
pub const PASSWORD: SecretWords = SecretWords {
    saved: "Password saved",
    clear: "Clear saved password",
    undo: "Undo clear",
    clear_to_change: "Clear the saved password to change it.",
    clear_on_save: "Save removes the saved password, or replaces it with the password you enter.",
};

/// What a secret row allows, from its saved, Clear and board state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretState {
    /// A saved secret is kept; the label says so and the input is read-only.
    pub kept: bool,
    /// A kept secret on a board that is on: the read-only input explains
    /// that Clear makes it editable.
    pub explains_clear: bool,
    pub input_enabled: bool,
    pub clear_enabled: bool,
}

impl SecretState {
    pub fn new(saved: bool, clear: bool, locked: bool) -> Self {
        let kept = saved && !clear;
        Self {
            kept,
            explains_clear: kept && !locked,
            input_enabled: !locked && !kept,
            clear_enabled: !locked && saved,
        }
    }

    /// Whether the row shows the Clear hint: its read-only input was clicked.
    pub fn is_hinted(self, clicked: bool) -> bool {
        self.explains_clear && clicked
    }

    /// The input's accessible label. The disabled input is not focusable,
    /// so its label carries the same explanation for keyboard and
    /// screen-reader users.
    pub fn input_label(self, label: &SharedString, words: &SecretWords) -> SharedString {
        if self.explains_clear {
            SharedString::from(format!(
                "{}, {}. {}",
                label,
                words.saved.to_lowercase(),
                words.clear_to_change
            ))
        } else {
            label.clone()
        }
    }
}

/// One secret row's facts. The owner keeps the input, the saved flag, the
/// Clear choice and which row last showed its hint.
pub struct SecretField<'a> {
    /// Identifies the row; its Clear button and input are scoped below it.
    pub id: ElementId,
    pub label: SharedString,
    pub input: &'a Entity<InputState>,
    pub clear: bool,
    pub saved: bool,
    /// The owner is turned off, so the whole row is read-only.
    pub locked: bool,
    /// The read-only input was clicked: show why it does not accept text.
    pub hinted: bool,
    pub words: &'static SecretWords,
}

/// Owner callbacks: toggle Clear, and show the hint for this row.
pub struct SecretActions<T, H> {
    pub on_toggle: T,
    pub on_hint: H,
}

pub fn secret_field<T, H>(
    field: SecretField<'_>,
    actions: SecretActions<T, H>,
    tokens: &Tokens,
) -> AnyElement
where
    T: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    H: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    let state = SecretState::new(field.saved, field.clear, field.locked);
    let hinted = state.is_hinted(field.hinted);
    let input_label = state.input_label(&field.label, field.words);
    let words = field.words;
    v_flex()
        .id(field.id.clone())
        .min_w_0()
        .gap_1()
        .child(label_row(&field, state, actions.on_toggle, tokens))
        .child(
            div()
                .id("saved-token-input")
                .test_support()
                // A disabled Input stops its own mouse-down, so listen in
                // the capture phase, before it reaches the input.
                .when(state.explains_clear, |input| {
                    input.capture_any_mouse_down(actions.on_hint)
                })
                .child(
                    Input::new(field.input)
                        .id("secret-input")
                        .aria_label(input_label)
                        .disabled(!state.input_enabled),
                ),
        )
        .when(hinted, |row| {
            row.child(
                crate::fields::note(tokens, words.clear_to_change)
                    .id("saved-token-hint")
                    .aria_label(words.clear_to_change)
                    .test_support(),
            )
        })
        .when(field.clear, |row| {
            row.child(div().text_color(tokens.yellow).child(words.clear_on_save))
        })
        .into_any_element()
}

/// The label, the saved state beside it, and the Clear button on the right.
fn label_row(
    field: &SecretField<'_>,
    state: SecretState,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    tokens: &Tokens,
) -> impl gpui_kit::IntoElement {
    let (icon, action) = if field.clear {
        (IconName::RotateCcw, field.words.undo)
    } else {
        (IconName::Eraser, field.words.clear)
    };
    h_flex()
        .gap_2()
        .items_center()
        .child(div().min_w_0().child(field.label.clone()))
        .when(state.kept, |row| {
            row.child(
                h_flex()
                    .id("saved-token-state")
                    .test_support()
                    .gap_1()
                    .text_sm()
                    .text_color(tokens.green)
                    .child(IconName::Lock)
                    .child(field.words.saved),
            )
        })
        .child(div().flex_1())
        .child(
            IconButton::new("clear-saved-token")
                .ghost()
                .icon(icon)
                .label(action)
                .disabled(!state.clear_enabled)
                .on_click(on_toggle),
        )
}

#[cfg(test)]
mod tests {
    use super::{PASSWORD, SecretState, TOKEN};
    use gpui_kit::SharedString;

    #[test]
    fn a_saved_secret_is_read_only_until_cleared() {
        let kept = SecretState::new(true, false, false);
        assert!(kept.kept && !kept.input_enabled && kept.clear_enabled);
        let cleared = SecretState::new(true, true, false);
        assert!(!cleared.kept && cleared.input_enabled && cleared.clear_enabled);
        let empty = SecretState::new(false, false, false);
        assert!(!empty.kept && empty.input_enabled && !empty.clear_enabled);
    }

    #[test]
    fn only_a_kept_secret_on_a_board_that_is_on_explains_clear() {
        let kept = SecretState::new(true, false, false);
        assert!(kept.explains_clear);
        assert_eq!(
            kept.input_label(&SharedString::from("Token"), &TOKEN)
                .as_ref(),
            "Token, token saved. Clear the saved token to change it."
        );
        assert_eq!(
            kept.input_label(&SharedString::from("Server password"), &PASSWORD)
                .as_ref(),
            "Server password, password saved. Clear the saved password to change it."
        );
        let locked = SecretState::new(true, false, true);
        assert!(!locked.explains_clear);
        assert_eq!(
            locked
                .input_label(&SharedString::from("Token"), &TOKEN)
                .as_ref(),
            "Token"
        );
    }

    #[test]
    fn a_turned_off_board_locks_the_whole_row() {
        for (saved, clear) in [(true, false), (true, true), (false, false)] {
            let state = SecretState::new(saved, clear, true);
            assert!(
                !state.input_enabled && !state.clear_enabled,
                "{saved} {clear}"
            );
        }
    }
}
