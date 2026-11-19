//! A saved token's field on a Settings page, through the shared secret row.

use gpui_kit::component::input::InputState;
use gpui_kit::{AnyElement, App, ClickEvent, Context, ElementId, Entity, SharedString, Window};
use subspace_dashboard::Tokens;
use subspace_dashboard::secret::{SecretActions, SecretField, TOKEN, secret_field};

use super::Settings;

pub(in crate::settings) struct SecretRow<'a> {
    /// Identifies the row; its Clear button and input are scoped below it.
    pub id: ElementId,
    pub label: SharedString,
    pub input: &'a Entity<InputState>,
    pub clear: bool,
    pub saved: bool,
    /// The board is turned off, so the whole row is read-only.
    pub locked: bool,
}

pub(in crate::settings) fn secret_row(
    row: SecretRow<'_>,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    tokens: &Tokens,
    settings: &Settings,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let hint = row.id.clone();
    secret_field(
        SecretField {
            hinted: settings.is_secret_hinted(&row.id),
            id: row.id,
            label: row.label,
            input: row.input,
            clear: row.clear,
            saved: row.saved,
            locked: row.locked,
            words: &TOKEN,
        },
        SecretActions {
            on_toggle,
            on_hint: cx.listener(move |settings, _, _, cx| {
                settings.show_secret_hint(hint.clone(), cx);
            }),
        },
        tokens,
    )
}
