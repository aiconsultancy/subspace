//! The board's keys (spec section 10). Plain keys act only while no text
//! field, menu or dialog has focus; `r`, `?`, F-keys and `Cmd+K` stay the
//! shell's. The board has no search adapter, so it binds `/` itself.

use gpui_kit::{App, Context, Div, Focusable as _, InteractiveElement as _, KeyBinding, actions};
use subspace_framework::dashboard::{KeyHelp, view_keys};

use crate::Issues;
use crate::model::Tab;

actions!(
    issues_board,
    [
        MyIssues,
        Active,
        Backlog,
        AllIssues,
        ToggleSubIssues,
        FocusSearch,
        LeaveSearch,
        BackToList,
        DismissDetail,
        NextIssue,
        PreviousIssue,
        Activity,
        FocusTimeline,
        CopyIdentifier,
        NewIssue,
        EditIssue
    ]
);

/// The key context when the issue page replaces the list.
pub(crate) const DETAIL_CONTEXT: &str = "IssueDetail";

/// The key context around the search field.
pub(crate) const SEARCH_CONTEXT: &str = "IssuesSearch";

pub(crate) const HELP: &[KeyHelp] = &[
    KeyHelp::new("j / ↓", "Next row"),
    KeyHelp::new("k / ↑", "Previous row"),
    KeyHelp::new("Enter", "Open issue or fold group"),
    KeyHelp::new("Esc", "Close the attachment, else back to issues"),
    KeyHelp::new("j / k", "Next or previous issue in detail"),
    KeyHelp::new("1–4", "My issues / Active / Backlog / All issues"),
    KeyHelp::new("5", "Issue activity"),
    KeyHelp::new("m", "Focus comments and activity"),
    KeyHelp::new("y / Cmd/Ctrl+.", "Copy issue identifier"),
    KeyHelp::new("Shift+S", "Show or hide sub-issues"),
    KeyHelp::new("c", "New issue"),
    KeyHelp::new("e", "Edit issue"),
    KeyHelp::new("⌘/Ctrl+Enter", "Save issue draft"),
    KeyHelp::new("/", "Search issues"),
];

pub(crate) fn bind(cx: &mut App) {
    let context = view_keys(crate::ID);
    let context = Some(context.as_str());
    let search = format!("{SEARCH_CONTEXT} > Input");
    let detail = view_keys(DETAIL_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("escape", DismissDetail, Some(&detail)),
        KeyBinding::new(
            "escape",
            subspace_framework::dashboard::find_in_page::Close,
            Some(&detail),
        ),
        KeyBinding::new("j", NextIssue, Some(&detail)),
        KeyBinding::new("k", PreviousIssue, Some(&detail)),
        KeyBinding::new("m", FocusTimeline, Some(&detail)),
        KeyBinding::new("y", CopyIdentifier, Some(&detail)),
        KeyBinding::new("cmd-.", CopyIdentifier, Some(&detail)),
        KeyBinding::new("ctrl-.", CopyIdentifier, Some(&detail)),
        KeyBinding::new("e", EditIssue, Some(&detail)),
        KeyBinding::new("c", NewIssue, context),
        KeyBinding::new("1", MyIssues, context),
        KeyBinding::new("2", Active, context),
        KeyBinding::new("3", Backlog, context),
        KeyBinding::new("4", AllIssues, context),
        KeyBinding::new("5", Activity, context),
        KeyBinding::new("shift-s", ToggleSubIssues, context),
        KeyBinding::new("/", FocusSearch, context),
        KeyBinding::new("escape", LeaveSearch, Some(search.as_str())),
        KeyBinding::new(
            "escape",
            subspace_framework::dashboard::find_in_page::Close,
            Some("(IssuesSearch > Input) && !SubspaceModal && !SubspaceRawKeys"),
        ),
        KeyBinding::new("enter", LeaveSearch, Some(search.as_str())),
    ]);
}

pub(crate) fn handlers(root: Div, cx: &mut Context<Issues>) -> Div {
    let root = editor_handlers(root, cx)
        .on_action(cx.listener(|this, _: &CopyIdentifier, _, cx| this.copy_issue_identifier(cx)))
        .on_action(cx.listener(|this, _: &MyIssues, _, cx| key_tab(this, Tab::MyIssues, cx)))
        .on_action(cx.listener(|this, _: &Active, _, cx| key_tab(this, Tab::Active, cx)))
        .on_action(cx.listener(|this, _: &Backlog, _, cx| key_tab(this, Tab::Backlog, cx)))
        .on_action(cx.listener(|this, _: &AllIssues, _, cx| key_tab(this, Tab::All, cx)))
        .on_action(cx.listener(|this, _: &Activity, window, cx| {
            if this.issue_view.snapshot.is_none() && this.is_ready() {
                this.open_activity(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &ToggleSubIssues, _, cx| {
            if !this.activity.visible && this.issue_view.snapshot.is_none() && this.is_ready() {
                this.toggle_sub_issues(cx);
            }
        }))
        .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
            if !this.activity.visible && this.issue_view.snapshot.is_none() && this.is_ready() {
                this.search.read(cx).focus_handle(cx).focus(window, cx);
            }
        }));
    navigation_handlers(root, cx)
}

fn editor_handlers(root: Div, cx: &mut Context<Issues>) -> Div {
    root.on_action(cx.listener(|this, _: &NewIssue, window, cx| {
        if this.can_create_issue() {
            this.open_issue_editor(false, window, cx);
        }
    }))
    .on_action(cx.listener(|this, _: &EditIssue, window, cx| {
        if this.can_edit_issue() {
            this.open_issue_editor(true, window, cx);
        }
    }))
}

fn navigation_handlers(root: Div, cx: &mut Context<Issues>) -> Div {
    root.on_action(cx.listener(|this, _: &BackToList, window, cx| {
        if this.menu.is_none() {
            this.go_back(window, cx);
        }
    }))
    // Escape closes the attachment panel first, the page's topmost layer.
    .on_action(cx.listener(|this, _: &DismissDetail, window, cx| {
        if this.menu.is_none() && !this.close_text(window, cx) {
            this.go_back(window, cx);
        }
    }))
    .on_action(cx.listener(|this, _: &NextIssue, window, cx| {
        this.focus_list(window, cx);
        this.step_issue(true, cx);
    }))
    .on_action(cx.listener(|this, _: &PreviousIssue, window, cx| {
        this.focus_list(window, cx);
        this.step_issue(false, cx);
    }))
    .on_action(cx.listener(|this, _: &FocusTimeline, window, cx| {
        this.focus_timeline(window, cx);
    }))
    .on_action(cx.listener(|this, _: &LeaveSearch, window, cx| {
        this.commit_search(cx);
        this.focus_list(window, cx);
    }))
}

/// A tab key acts only while the tabs show.
fn key_tab(issues: &mut Issues, tab: Tab, cx: &mut Context<Issues>) {
    if issues.issue_view.snapshot.is_none() && issues.is_ready() {
        issues.set_tab(tab, cx);
    }
}
