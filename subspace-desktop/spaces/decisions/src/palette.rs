//! Palette access to everything the navigator, the toolbar and the
//! inspector offer; no reads of its own.

use gpui_kit::{App, Context, SharedString, Window};
use subspace_framework::dashboard::{CloseDetail, PaletteCommand};

use crate::keys;
use crate::menus::{self, Kind};
use crate::model::{Maker, ProjectChoice, View};
use crate::{Decisions, open_issue, words};

/// The palette lists at most this many linked issues to open.
const MAX_ISSUE_COMMANDS: usize = 10;
/// The views' keys, in the navigator's order.
const VIEW_KEYS: [&str; 5] = ["1", "2", "3", "4", "5"];

/// The Space's commands in the palette.
pub(crate) fn commands(this: &Decisions) -> Vec<PaletteCommand> {
    let mut commands = Vec::new();
    // Nothing read for an account that is no longer signed in is offered.
    let moved = this.account_moved();
    if !moved {
        commands.extend(decision_commands(this));
    }
    let search = PaletteCommand::new("Search decisions", keys::FocusSearch);
    commands.push(search.shortcut("/"));
    commands.extend(view_commands(this));
    commands.extend(maker_commands(this));
    if !moved {
        commands.extend(project_commands(this));
    }
    commands.push(callback(this, "Filter by tag…", |this, window, cx| {
        menus::open(this, Kind::Tag, window, cx);
    }));
    commands.extend(dock_commands(this));
    commands
}

/// Open or close the decision, then open its linked issues.
fn decision_commands(this: &Decisions) -> Vec<PaletteCommand> {
    let mut commands = Vec::new();
    if this.shown_page().is_some() {
        let close = PaletteCommand::new("Close the decision", CloseDetail);
        commands.push(close.shortcut("Esc"));
    } else if this.selected_item().is_some() {
        let open = PaletteCommand::new("Open decision", keys::OpenDecision);
        commands.push(open.shortcut("Enter"));
    }
    commands.extend(issue_commands(this));
    commands
}

/// A command that runs `run` on the Space.
fn callback(
    this: &Decisions,
    label: impl Into<SharedString>,
    run: impl Fn(&mut Decisions, &mut Window, &mut Context<Decisions>) + 'static,
) -> PaletteCommand {
    let weak = this.weak.clone();
    PaletteCommand::callback(label, move |window: &mut Window, cx: &mut App| {
        _ = weak.update(cx, |this, cx| run(this, window, cx));
    })
}

fn view_commands(this: &Decisions) -> Vec<PaletteCommand> {
    View::ALL
        .into_iter()
        .zip(VIEW_KEYS)
        .map(|(view, key)| {
            callback(
                this,
                format!("View: {}", view.label()),
                move |this, _, cx| {
                    this.choose_view(view, cx);
                },
            )
            .shortcut(key)
        })
        .collect()
}

fn maker_commands(this: &Decisions) -> Vec<PaletteCommand> {
    Maker::ALL
        .into_iter()
        .map(|maker| {
            callback(
                this,
                format!("Made by: {}", maker.label()),
                move |this, _, cx| {
                    this.choose_maker(maker, cx);
                },
            )
        })
        .collect()
}

fn project_commands(this: &Decisions) -> Vec<PaletteCommand> {
    let every = std::iter::once((ProjectChoice::All, "All projects".to_owned()));
    let active = this
        .session
        .projects
        .iter()
        .filter(|project| project.archived_at.is_none())
        .map(|project| {
            let label = words::project(&project.key, Some(&project.name));
            (ProjectChoice::Project(project.key.clone()), label)
        });
    every
        .chain(active)
        .map(|(project, label)| {
            callback(this, format!("Project: {label}"), move |this, _, cx| {
                this.choose_project(project.clone(), cx);
            })
        })
        .collect()
}

/// Open each issue the selected or open decision links to.
fn issue_commands(this: &Decisions) -> Vec<PaletteCommand> {
    let linked = match this.shown_page() {
        Some(_) => this.page_issues(),
        None => this
            .selected_item()
            .map(|item| crate::peek::relations(item).issues)
            .unwrap_or_default(),
    };
    linked
        .into_iter()
        .take(MAX_ISSUE_COMMANDS)
        .map(|issue| {
            let identifier = issue.identifier;
            callback(
                this,
                format!("Open issue {identifier}"),
                move |this, _, cx| {
                    open_issue::open(&identifier, this, cx);
                },
            )
        })
        .collect()
}

fn dock_commands(this: &Decisions) -> [PaletteCommand; 2] {
    let navigator = if this.docks.navigator {
        "Hide navigator"
    } else {
        "Show navigator"
    };
    let inspector = if this.docks.inspector {
        "Hide inspector"
    } else {
        "Show inspector"
    };
    [
        callback(this, navigator, |this, _, cx| this.toggle_navigator(cx)),
        callback(this, inspector, |this, _, cx| this.toggle_inspector(cx)),
    ]
}
