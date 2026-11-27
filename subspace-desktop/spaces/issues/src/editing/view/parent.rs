//! The Parent field uses the shared anchored menu. Only the draft's parent
//! value is saved; search text and continuation state die with the menu.
use std::time::Duration;

use gpui_kit::component::Disableable as _;
use gpui_kit::{AnyElement, AppContext as _, Context, Entity, IntoElement as _, Task, Window};
use subspace_api_client::{
    issue_writes::IssueSaveError,
    model::{IssueListItem, Page},
};
use subspace_framework::dashboard::{
    action::menu_button,
    fields::{self, InputRole},
    menu::{Menu, MenuContents, MenuEvent, MenuOption},
};
use subspace_framework::lucide::IconName;

use crate::{Issues, editing::IssueEditor};

const ANCHOR: &str = "issue-parent-picker";
const CONTINUE: &str = ":continue";
const DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Default)]
pub(in crate::editing) struct ParentSearch {
    query: String,
    items: Vec<IssueListItem>,
    next: Option<String>,
    failed: bool,
    _task: Option<Task<()>>,
}

pub(in crate::editing) fn searches_trackers() -> bool {
    subspace_api_client::capabilities().issue_parent_search
}

pub(super) fn control(
    board: &Issues,
    editor: &IssueEditor,
    text_field: (InputRole, &'static str),
    locked: bool,
    cx: &mut Context<Issues>,
) -> AnyElement {
    if !searches_trackers() {
        let (role, input_id) = text_field;
        return fields::input(&editor.parent, role, "Parent issue identifier")
            .id(input_id)
            .disabled(locked)
            .into_any_element();
    }
    let selected = editor.parent.read(cx).value();
    let label = if selected.is_empty() {
        "No parent"
    } else {
        selected.as_str()
    };
    let button = menu_button(
        ANCHOR,
        IconName::ListTree,
        label.to_owned(),
        !selected.is_empty(),
    )
    .accessibility_label(format!("Parent: {label}"))
    .disabled(locked)
    .on_click(cx.listener(|board, _, window, cx| open(board, window, cx)));
    board
        .menu_anchors
        .trigger(ANCHOR, button, cx)
        .into_any_element()
}

pub(in crate::editing) fn open(board: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    let Some(editor) = &mut board.editor else {
        return;
    };
    if editor.controls_locked() || !searches_trackers() {
        return;
    }
    let contents = MenuContents::new("Parent", "Find a tracker by key or title", vec![])
        .owner_filtered(true)
        .keep_open(true);
    let anchor = board.menu_anchors.anchor(ANCHOR, cx);
    let Some(menu) = board.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    editor.parent_search = Some(ParentSearch::default());
    cx.subscribe_in(
        &menu,
        window,
        |board, menu, event: &MenuEvent, window, cx| {
            if board.menu.as_ref() != Some(menu) {
                return;
            }
            match event {
                MenuEvent::Query(query) => search(board, menu, Some(query.as_str()), cx),
                MenuEvent::Pick(value) if value.as_str() == CONTINUE => {
                    search(board, menu, None, cx)
                }
                MenuEvent::Pick(value) => pick(board, value.as_str(), window, cx),
                MenuEvent::Close => close(board, cx),
            }
        },
    )
    .detach();
    board.menu = Some(menu.clone());
    search(board, &menu, Some(""), cx);
    cx.notify();
}

fn close(board: &mut Issues, cx: &mut Context<Issues>) {
    board.menu = None;
    if let Some(editor) = &mut board.editor {
        editor.parent_search = None;
    }
    cx.notify();
}

fn pick(board: &mut Issues, value: &str, window: &mut Window, cx: &mut Context<Issues>) {
    let Some(editor) = board
        .editor
        .as_ref()
        .filter(|editor| !editor.controls_locked())
    else {
        return;
    };
    let valid = value.is_empty()
        || editor.parent_search.as_ref().is_some_and(|search| {
            search
                .items
                .iter()
                .any(|item| item.fields.identifier == value)
        });
    if valid {
        editor.parent.update(cx, |input, cx| {
            input.set_value(value.to_owned(), window, cx)
        });
        board.menu_anchors.dismiss(window, cx);
        close(board, cx);
        board.editor_changed(cx);
    }
}

fn search(board: &mut Issues, menu: &Entity<Menu>, query: Option<&str>, cx: &mut Context<Issues>) {
    let (Some(hub), Some(editor)) = (board.hub.clone(), board.editor.as_mut()) else {
        return;
    };
    let Some(search) = &mut editor.parent_search else {
        return;
    };
    if let Some(query) = query {
        *search = ParentSearch {
            query: query.to_owned(),
            ..ParentSearch::default()
        };
    }
    update_menu(menu, &editor.parent.read(cx).value(), search, true, cx);
    let scope = editor.scope.clone();
    let exclude = editor.basis.as_ref().map(|issue| issue.fields.id.clone());
    let key = editor.key.value();
    let menu = menu.downgrade();
    let (text, cursor) = (search.query.clone(), search.next.clone());
    let debounce = query.is_some_and(|query| !query.is_empty());
    search._task = Some(cx.spawn(async move |board, cx| {
        if debounce {
            cx.background_executor().timer(DEBOUNCE).await;
        }
        let result = cx
            .background_spawn(async move {
                hub.tracker_parents(&scope, &text, cursor, exclude.as_deref())
            })
            .await;
        let _ = board.update(cx, |board, cx| {
            if let Some(menu) = menu.upgrade()
                && board.menu.as_ref() == Some(&menu)
                && let Some(editor) = board.current_editor(key)
            {
                loaded(editor, &menu, result, cx);
            }
        });
    }));
}

fn loaded(
    editor: &mut IssueEditor,
    menu: &Entity<Menu>,
    result: Result<Page<IssueListItem>, IssueSaveError>,
    cx: &mut Context<Issues>,
) {
    let Some(search) = &mut editor.parent_search else {
        return;
    };
    search.failed = result.is_err();
    if let Ok(page) = result {
        search.items.extend(page.items);
        search.next = page.next_cursor;
    }
    update_menu(menu, &editor.parent.read(cx).value(), search, false, cx);
}

fn update_menu(
    menu: &Entity<Menu>,
    selected: &str,
    search: &ParentSearch,
    loading: bool,
    cx: &mut Context<Issues>,
) {
    let (options, note) = menu_options(selected, search, loading);
    menu.update(cx, |menu, cx| {
        menu.set_options(options, cx);
        menu.set_note(note.map(Into::into), cx);
    });
}

fn menu_options(
    selected: &str,
    search: &ParentSearch,
    loading: bool,
) -> (Vec<MenuOption>, Option<&'static str>) {
    let mut options = vec![MenuOption::new("", "No parent").checked(selected.is_empty())];
    options.extend(search.items.iter().map(|item| {
        let fields = &item.fields;
        MenuOption::new(
            fields.identifier.clone(),
            format!(
                "{} · {} · {}",
                fields.identifier, fields.title, fields.state.name
            ),
        )
        .checked(fields.identifier == selected)
    }));
    let note = if loading {
        Some("Searching trackers…")
    } else if search.failed {
        options.push(MenuOption::new(CONTINUE, "Retry search"));
        Some("Couldn't search trackers")
    } else if search.next.is_some() {
        options.push(MenuOption::new(CONTINUE, "Load more trackers"));
        None
    } else if search.items.is_empty() {
        Some("No matching trackers")
    } else {
        None
    };
    (options, note)
}

#[cfg(test)]
impl Issues {
    pub(crate) fn test_parent_options(&self) -> Vec<String> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.parent_search.as_ref())
            .map(|search| {
                menu_options("", search, false)
                    .0
                    .into_iter()
                    .map(|option| option.label.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }
}
