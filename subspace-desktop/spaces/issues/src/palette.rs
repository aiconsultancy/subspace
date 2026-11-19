//! Palette access to the board's existing actions; no additional data reads.

use subspace_api_client::hub::{IssueSnapshot, IssueStatus, ReadError};
use subspace_framework::dashboard::{PaletteCommand, PaletteOption, SubList, links};

use crate::Issues;
use crate::keys;
use crate::model::{ProjectScope, SubIssues};
use crate::scope::{self, Body, ScreenAction};

// qual:allow(coupling, oi) reason: "The board owns its palette composition and callbacks; its other state and view concerns remain in their existing modules."
impl Issues {
    pub(crate) fn palette_commands(&self) -> Vec<PaletteCommand> {
        let mut commands = Vec::new();
        if self.editor.is_some() {
            return commands;
        }
        if self.can_create_issue() {
            commands.push(PaletteCommand::new("New issue…", keys::NewIssue).shortcut("c"));
        }
        if self.can_edit_issue() {
            commands.push(PaletteCommand::new("Edit issue…", keys::EditIssue).shortcut("e"));
        }
        if let Some(issue) = &self.issue_view.snapshot {
            commands.extend(self.detail_commands(issue));
            return commands;
        }
        let body = scope::body(&self.access, &self.listing);
        if self.activity.visible && self.is_ready() {
            commands.extend(tab_commands());
            commands.push(self.project_command());
            commands.extend(self.activity_row_command());
            return commands;
        }
        if body.controls() {
            commands.extend(self.list_commands());
        }
        if let Body::Screen { screen, .. } = body {
            commands.extend(self.recovery_commands(&screen));
        } else if self.listing.offline().is_some() {
            commands.push(self.retry_command());
        }
        commands.extend(self.row_commands());
        commands
    }

    fn detail_commands(&self, issue: &IssueSnapshot) -> Vec<PaletteCommand> {
        let mut back = PaletteCommand::new(self.back.label(), keys::BackToList).shortcut("Esc");
        // Recent commands keep one static entry, not a channel's name.
        back.recent_key = Some(crate::back::TO_LIST.into());
        let mut commands = vec![back];
        commands.push(PaletteCommand::new("Copy identifier", keys::CopyIdentifier).shortcut("y"));
        commands.extend(self.related_commands(issue));
        commands.extend(crate::navigation::commands(issue, &self.weak));
        match issue.status() {
            IssueStatus::Loaded => {
                commands.extend(self.timeline_commands());
                if issue
                    .detail()
                    .and_then(|detail| detail.children.as_ref())
                    .is_some_and(|page| page.next_cursor.is_some())
                    && !issue.children_loading()
                {
                    let label = if issue.children_failed() {
                        "Retry sub-issues"
                    } else {
                        "Show more sub-issues"
                    };
                    let weak = self.weak.clone();
                    commands.push(PaletteCommand::callback(label, move |window, cx| {
                        _ = weak.update(cx, |this, cx| {
                            this.focus_list(window, cx);
                            this.more_subissues(cx);
                        });
                    }));
                }
            }
            IssueStatus::Error(ReadError::Network) => commands.push(self.retry_command()),
            IssueStatus::Error(reason) => commands.extend(
                self.recovery_commands(&scope::Screen::Failed(crate::sync::failure(reason))),
            ),
            IssueStatus::Loading | IssueStatus::Unavailable => {}
        }
        commands
    }

    fn related_commands(&self, issue: &IssueSnapshot) -> Vec<PaletteCommand> {
        if *issue.status() != IssueStatus::Loaded {
            return Vec::new();
        }
        let relations = issue
            .detail()
            .and_then(|d| d.relations.as_ref())
            .is_some_and(|p| p.next_cursor.is_some());
        let prs = !issue.pull_requests_unavailable()
            && (issue.pull_requests_failed()
                || issue
                    .pull_requests()
                    .is_some_and(|p| p.next_cursor.is_some()));
        [
            (
                relations && !issue.relations_loading(),
                if issue.relations_failed() {
                    "Retry relations"
                } else {
                    "Show more relations"
                },
                crate::requests::Request::MoreRelations,
            ),
            (
                prs && !issue.pull_requests_loading(),
                if issue.pull_requests_failed() {
                    "Retry pull requests"
                } else {
                    "Show more pull requests"
                },
                crate::requests::Request::MorePullRequests,
            ),
        ]
        .into_iter()
        .filter(|(available, _, _)| *available)
        .map(|(_, label, request)| {
            let weak = self.weak.clone();
            PaletteCommand::callback(label, move |_, cx| {
                _ = weak.update(cx, |this, cx| this.request(request.clone(), cx));
            })
        })
        .collect()
    }

    fn timeline_commands(&self) -> Vec<PaletteCommand> {
        use crate::detail::timeline::TimelineMode;
        let mode = self.issue_view.timeline.mode;
        let (label, next) = match mode {
            TimelineMode::Comments => ("Show all issue activity", TimelineMode::All),
            TimelineMode::All => ("Show issue comments", TimelineMode::Comments),
        };
        let weak = self.weak.clone();
        vec![
            PaletteCommand::new("Focus discussion", keys::FocusTimeline).shortcut("m"),
            PaletteCommand::callback(label, move |window, cx| {
                _ = weak.update(cx, |this, cx| {
                    this.show_timeline(next, cx);
                    this.focus_timeline(window, cx);
                });
            }),
        ]
    }

    fn list_commands(&self) -> Vec<PaletteCommand> {
        let subissues = match self.sub_issues {
            SubIssues::Hidden => "Show sub-issues",
            SubIssues::Flat => "Hide sub-issues",
        };
        let mut commands = tab_commands().to_vec();
        commands.extend([
            PaletteCommand::new(subissues, keys::ToggleSubIssues).shortcut("Shift+S"),
            PaletteCommand::new("Search issues", keys::FocusSearch).shortcut("/"),
            self.project_command(),
            self.filter_command(crate::filter_menu::FilterMenu::Assignee, "Filter assignee…"),
            self.filter_command(crate::filter_menu::FilterMenu::Priority, "Filter priority…"),
            self.filter_command(crate::filter_menu::FilterMenu::More, "Issue filters…"),
        ]);
        commands
    }

    fn activity_row_command(&self) -> Option<PaletteCommand> {
        let key = self.activity.list.selected()?.clone();
        let label = self.activity.action_name(&key)?;
        let weak = self.weak.clone();
        let command = PaletteCommand::callback(label, move |window, cx| {
            _ = weak.update(cx, |board, cx| {
                board.focus_list(window, cx);
                board.activate_activity_row(&key, cx);
            });
        });
        Some(command.shortcut("Enter"))
    }

    fn filter_command(&self, menu: crate::filter_menu::FilterMenu, label: &str) -> PaletteCommand {
        let weak = self.weak.clone();
        PaletteCommand::callback(label.to_owned(), move |window, cx| {
            _ = weak.update(cx, |this, cx| {
                crate::filter_menu::open(menu, this, window, cx)
            });
        })
    }

    fn project_command(&self) -> PaletteCommand {
        let options = crate::pickers::scoped_options(self)
            .map(|(project, label)| {
                PaletteOption::new(project.key().to_owned(), label).checked(project == self.project)
            })
            .collect();
        let weak = self.weak.clone();
        PaletteCommand::sub(
            "Switch project…",
            SubList::new("Project", options, move |project, _, cx| {
                _ = weak.update(cx, |this, cx| {
                    this.set_project(ProjectScope::from_key(project), cx)
                });
            }),
        )
    }

    fn recovery_commands(&self, screen: &scope::Screen) -> Vec<PaletteCommand> {
        scope::screen_actions(screen)
            .into_iter()
            .filter_map(|action| match action {
                ScreenAction::Retry => Some(self.retry_command()),
                ScreenAction::CopyDetails => None,
            })
            .collect()
    }

    fn retry_command(&self) -> PaletteCommand {
        let weak = self.weak.clone();
        PaletteCommand::callback("Retry", move |_, cx| {
            _ = weak.update(cx, |this, cx| {
                if this.issue_view.snapshot.is_some() {
                    this.reload_detail(cx);
                } else {
                    this.retry(cx);
                }
            });
        })
    }

    fn row_commands(&self) -> Vec<PaletteCommand> {
        let Some(item) = self.list.selected().and_then(|key| self.issue(key)) else {
            return Vec::new();
        };
        let weak = self.weak.clone();
        let key = self.list.selected().cloned();
        let open = PaletteCommand::callback("Open issue", move |window, cx| {
            if let Some(key) = &key {
                _ = weak.update(cx, |this, cx| {
                    this.focus_list(window, cx);
                    this.open_row(key, cx);
                });
            }
        })
        .shortcut("Enter");
        let copy_commands = [
            ("Copy title", &item.fields.title),
            ("Copy id", &item.fields.identifier),
        ]
        .into_iter()
        .map(|(label, text)| {
            let text = text.clone();
            PaletteCommand::callback(label, move |_, cx| links::copy(text.clone(), cx))
        });
        std::iter::once(open).chain(copy_commands).collect()
    }
}

fn tab_commands() -> [PaletteCommand; 5] {
    [
        PaletteCommand::new("My issues", keys::MyIssues).shortcut("1"),
        PaletteCommand::new("Active issues", keys::Active).shortcut("2"),
        PaletteCommand::new("Backlog issues", keys::Backlog).shortcut("3"),
        PaletteCommand::new("All issues", keys::AllIssues).shortcut("4"),
        PaletteCommand::new("Issue activity", keys::Activity).shortcut("5"),
    ]
}
