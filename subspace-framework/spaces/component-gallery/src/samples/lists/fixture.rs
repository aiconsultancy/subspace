//! A fixture list view for the `list::render` sample. The shared list
//! renderer draws a `ListDashboard`, so this small view implements one with
//! fixture groups and no details, which keeps the detail panel (and its
//! app-wide width preference) closed. The muted row stands for one a
//! click only selects and a double click activates, such as a channel not
//! joined yet.
//! A second instance adds group notes.

use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render, SharedString, Window,
};
use subspace_dashboard::list::{self, Detail, Group, GroupNote, ListDashboard, ListState, Row};
use subspace_dashboard::{Dashboard, DashboardEvent, Glyph, Tone};

/// The muted row: a click selects it and a double click activates it.
const MUTED_ROW: &str = "gallery-row-4";

/// The empty groups of the notes sample: one still loading, one loaded with
/// nothing to show.
const NOTE_GROUPS: [(&str, bool, &str); 2] = [
    ("example/loading", true, "Loading fixture rows…"),
    ("example/archive", false, "No open fixture rows"),
];

/// The groups and rows of the fixture list: a group, then its rows.
const GROUPS: [(&str, &str, [(&str, &str, &str); 2]); 2] = [
    (
        "example/component-gallery",
        "▣",
        [
            ("gallery-row-1", "#12", "Add a copy button to every card"),
            ("gallery-row-2", "#13", "Hide unused GPUI Kit components"),
        ],
    ),
    (
        "example/fixtures",
        "▣",
        [
            ("gallery-row-3", "#4", "Refresh the fixture data"),
            ("gallery-row-4", "#5", "Name every sample state"),
        ],
    ),
];

fn row((key, id, title): (&str, &str, &str)) -> Row {
    Row {
        key: key.into(),
        icon: Glyph::new("●", Tone::Green),
        id: Some(id.into()),
        title: title.into(),
        trailing: Vec::new(),
        done: key == MUTED_ROW,
        indent: 0,
    }
}

fn groups() -> Vec<Group> {
    GROUPS
        .iter()
        .map(|(name, icon, rows)| Group {
            key: (*name).into(),
            icon: Glyph::new(*icon, Tone::Muted),
            name: (*name).into(),
            count: Some(rows.len().to_string().into()),
            rows: rows.iter().copied().map(row).collect(),
        })
        .collect()
}

/// A group with one row, then the note groups with no rows. Its keys differ
/// from the first sample's, so the two lists share no element identity.
fn note_groups() -> (Vec<Group>, Vec<(SharedString, GroupNote)>) {
    let mut groups = vec![Group {
        key: "example/notes".into(),
        icon: Glyph::new("▣", Tone::Muted),
        name: "example/notes".into(),
        count: Some("1".into()),
        rows: vec![row(("gallery-note-row-1", "#21", "Explain an empty group"))],
    }];
    let mut notes = Vec::new();
    for (name, loading, text) in NOTE_GROUPS {
        groups.push(Group {
            key: name.into(),
            icon: Glyph::new("▣", Tone::Muted),
            name: name.into(),
            count: Some("0".into()),
            rows: Vec::new(),
        });
        let note = if loading {
            GroupNote::Loading(text.into())
        } else {
            GroupNote::Empty(text.into())
        };
        notes.push((name.into(), note));
    }
    (groups, notes)
}

/// The fixture list view.
pub(crate) struct FixtureList {
    focus: FocusHandle,
    list: ListState,
    notes: Vec<(SharedString, GroupNote)>,
}

impl FixtureList {
    /// The fixture list with a loading and an empty group note.
    pub(crate) fn with_notes(cx: &mut Context<Self>) -> Self {
        let (groups, notes) = note_groups();
        Self {
            focus: cx.focus_handle(),
            list: ListState::new(groups),
            notes,
        }
    }
}

impl EventEmitter<DashboardEvent> for FixtureList {}

impl Focusable for FixtureList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for FixtureList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        list::render(self, window, cx)
    }
}

impl Dashboard for FixtureList {
    const ID: &'static str = "component-gallery-fixture-list";
    const TITLE: &'static str = "Fixture list";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            list: ListState::new(groups()),
            notes: Vec::new(),
        }
    }

    fn reload(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }
}

impl ListDashboard for FixtureList {
    fn list(&self) -> &ListState {
        &self.list
    }

    fn list_mut(&mut self) -> &mut ListState {
        &mut self.list
    }

    fn detail(&self, _: &str) -> Option<Detail> {
        None
    }

    fn activates_on_click(&self, row: &Row) -> bool {
        row.key != MUTED_ROW
    }

    fn group_note(&self, group: &Group, _: &App) -> Option<GroupNote> {
        self.notes
            .iter()
            .find(|(key, _)| *key == group.key)
            .map(|(_, note)| note.clone())
    }
}
