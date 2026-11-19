//! A board's run-time settings on its Settings page.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, IntoElement as _, ParentElement as _, SharedString, Styled as _, div,
};
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::toggle::toggle_row;
use subspace_dashboard::{Tokens, anchored};
use subspace_lucide::IconName;

use super::{SecretRow, labelled, note, secret_row};
use crate::settings::Settings;
use crate::settings::board_fields::{AppSecrets, Control, Field, app_secrets_text};

/// Where a field row sits: its board's id and draft index, the field's
/// index, and whether the board is turned off.
#[derive(Clone, Copy)]
struct Place<'a> {
    board_id: &'a str,
    board: usize,
    index: usize,
    locked: bool,
}

impl Settings {
    /// The notes and fields of board `id`, when it declares any. `locked`
    /// makes them read-only while the board is turned off.
    pub(in crate::settings) fn board_field_rows(
        &self,
        id: &str,
        locked: bool,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let board = self.board_fields.iter().position(|draft| draft.id == id)?;
        let draft = &self.board_fields[board];
        let notes = draft.notes.iter().map(|text| note(tokens, text.clone()));
        let place = |index| Place {
            board_id: id,
            board,
            index,
            locked,
        };
        let rows: Vec<AnyElement> = draft
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| field_row(self, place(index), field, tokens, cx))
            .collect();
        let app_secrets = draft
            .app_secrets
            .as_ref()
            .filter(|app| !app.keys.is_empty())
            .map(|app| app_secrets_row(place(0), app, tokens, cx));
        let grants = draft.permissions.iter().map(|grant| {
            let key = grant.key.clone();
            toggle_row(
                grant.label.clone(),
                subspace_dashboard::toggle::switch(
                    SharedString::from(format!("board-grant-{id}-{key}")),
                    cx,
                )
                .checked(grant.on)
                .disabled(locked)
                .on_click(cx.listener(move |settings, on: &bool, _, cx| {
                    settings.set_board_grant(board, &key, *on);
                    cx.notify();
                })),
            )
        });
        Some(
            v_flex()
                .gap_3()
                .children(notes)
                .children(grants)
                .children(rows)
                .children(app_secrets)
                .into_any_element(),
        )
    }
}

fn field_row(
    settings: &Settings,
    place: Place<'_>,
    field: &Field,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let Place {
        board_id,
        board,
        index,
        locked,
    } = place;
    let label = SharedString::from(field.spec.label.clone());
    let id = SharedString::from(format!("board-field-{board_id}-{}", field.spec.key));
    let control = match &field.control {
        Control::Text { input, .. } => labelled(
            label.clone(),
            Input::new(input).aria_label(label).disabled(locked),
        )
        .into_any_element(),
        Control::Toggle { on, .. } => toggle_row(
            label,
            subspace_dashboard::toggle::switch(id, cx)
                .checked(*on)
                .disabled(locked)
                .on_click(cx.listener(move |settings, on: &bool, _, cx| {
                    settings.set_field_toggle(board, index, *on);
                    cx.notify();
                })),
        )
        .into_any_element(),
        Control::Choice { choice, .. } => labelled(
            label.clone(),
            anchored::select(choice)
                .accessibility_label(label)
                .disabled(locked),
        )
        .into_any_element(),
        Control::Secret {
            input,
            clear,
            saved,
        } => secret_row(
            SecretRow {
                id: id.into(),
                label,
                input,
                clear: *clear,
                saved: *saved,
                locked,
            },
            cx.listener(move |settings, _, window, cx| {
                settings.toggle_field_clear(board, index, window, cx);
            }),
            tokens,
            settings,
            cx,
        ),
    };
    with_field_help(control, field, tokens)
}

/// "This app has saved 3 secrets", read-only, with Clear (applied on Save).
fn app_secrets_row(
    place: Place<'_>,
    app: &AppSecrets,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let Place {
        board_id,
        board,
        locked,
        ..
    } = place;
    let (icon, action) = if app.clear {
        (IconName::RotateCcw, "Undo clear")
    } else {
        (IconName::Eraser, "Clear")
    };
    v_flex()
        .gap_1()
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(app_secrets_text(app.keys.len())),
                )
                .child(
                    IconButton::new(SharedString::from(format!("board-app-secrets-{board_id}")))
                        .ghost()
                        .icon(icon)
                        .label(action)
                        .disabled(locked)
                        .on_click(cx.listener(move |settings, _, _, cx| {
                            settings.toggle_app_secrets_clear(board, cx);
                        })),
                ),
        )
        .when(app.clear, |row| {
            row.child(
                div()
                    .text_color(tokens.yellow)
                    .child("Save removes every secret this app saved."),
            )
        })
        .into_any_element()
}

fn with_field_help(control: AnyElement, field: &Field, tokens: &Tokens) -> AnyElement {
    v_flex()
        .gap_1()
        .child(control)
        .children(field.spec.help.clone().map(|help| note(tokens, help)))
        .into_any_element()
}
