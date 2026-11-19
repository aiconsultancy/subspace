use gpui_kit::component::Disableable as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, Context, Entity, IntoElement as _, Window};
mod loading;
mod options;

use loading::load;
use options::{available, display_selection, options};
use subspace_framework::dashboard::{
    action::menu_button,
    fields,
    menu::{Menu, MenuContents, MenuEvent},
};

use super::super::{Field, IssueEditor};
use crate::Issues;

pub(super) fn render(
    board: &Issues,
    editor: &IssueEditor,
    field: Field,
    locked: bool,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let (_, label, _) = field.facts();
    fields::form_field(label, control(board, editor, field, locked, cx)).into_any_element()
}

pub(super) fn control(
    board: &Issues,
    editor: &IssueEditor,
    field: Field,
    locked: bool,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let (id, label, icon) = field.facts();
    let selected = editor.properties.selected(field);
    let value = display_selection(board, editor, field);
    let reason = disabled_reason(editor, field);
    let button = menu_button(id, icon, value.clone(), !selected.is_empty())
        .accessibility_label(format!("{label}: {value}"))
        .disabled(locked || reason.is_some())
        .when_some(reason, |button, reason| button.tooltip(reason))
        .on_click(cx.listener(move |board, _, window, cx| open(board, field, window, cx)));
    board
        .menu_anchors
        .trigger(id, button, cx)
        .into_any_element()
}

fn disabled_reason(editor: &IssueEditor, field: Field) -> Option<&'static str> {
    match field {
        Field::Project if editor.editing_existing() => {
            Some("Move the issue from Properties after saving or discarding this draft.")
        }
        Field::State if editor.properties.tracker() => {
            Some("Tracker state is derived from its children.")
        }
        Field::Cycle if editor.properties.tracker() => {
            Some("Trackers cannot join a cycle directly.")
        }
        Field::Estimate if editor.properties.tracker() => {
            Some("Trackers derive progress from children and accept no direct estimate.")
        }
        _ => None,
    }
}

pub(crate) fn open(
    board: &mut Issues,
    field: Field,
    window: &mut Window,
    cx: &mut Context<Issues>,
) {
    let Some(editor) = &board.editor else {
        return;
    };
    if editor.frozen() || disabled_reason(editor, field).is_some() {
        return;
    }
    let (id, label, _) = field.facts();
    let kept = field == Field::Labels;
    let options = options(board, editor, field);
    let contents =
        MenuContents::new(label, format!("Find {}", label.to_lowercase()), options).keep_open(kept);
    let anchor = board.menu_anchors.anchor(id, cx);
    let Some(menu) = board.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    let key = editor.key.value();
    cx.subscribe(&menu, move |board, menu, event: &MenuEvent, cx| {
        let Some(editor) = board.editor.as_ref() else {
            return;
        };
        if editor.key.value() != key {
            return;
        }
        match event {
            MenuEvent::Pick(value) => pick(board, field, value, &menu, cx),
            MenuEvent::Close => board.menu = None,
            MenuEvent::Query(_) => {}
        }
        cx.notify();
    })
    .detach();
    board.menu = Some(menu.clone());
    load(board, editor, menu, field, cx);
    cx.notify();
}

fn pick(
    board: &mut Issues,
    field: Field,
    value: &str,
    menu: &Entity<Menu>,
    cx: &mut Context<Issues>,
) {
    let Some(editor) = board.editor.as_ref() else {
        return;
    };
    let choice = if value.is_empty() && field.can_clear() {
        None
    } else {
        let Some(choice) = available(board, editor, field)
            .into_iter()
            .chain(editor.properties.selected(field).iter().cloned())
            .find(|choice| choice.id() == value)
        else {
            return;
        };
        Some(choice)
    };
    if let Some(editor) = &mut board.editor {
        editor.properties.pick(field, choice);
    }
    board.editor_changed(cx);
    if field == Field::Labels {
        if let Some(editor) = &board.editor {
            let options = options(board, editor, field);
            menu.update(cx, |menu, cx| menu.set_options(options, cx));
        }
    } else {
        board.menu = None;
    }
}
