//! The Copilot Agents commands the command palette offers: the filters, the actions
//! on the selected row, and the alert settings.

use gpui_kit::{App, Context};
use subspace_framework::dashboard::{PaletteCommand, PaletteOption, SubList, alerts};

use crate::CopilotAgents;
use crate::keys::{
    ClearFilters, CopyLink, OpenInChat, OpenPullRequest, OpenSession, OpenSubject, Picking,
    ToggleMonitor,
};
use crate::logs::log_commands;
use crate::model::Kind;
use crate::ui::{menu_title, monitor_command, shortcut, toggle_label};

/// The sounds the alert menu offers. All are macOS system sounds.
const SOUNDS: [&str; 5] = ["Glass", "Ping", "Submarine", "Blow", "Funk"];

// qual:allow(coupling, oi) reason: "Palette commands read the selected Copilot Agents row and apply filters through the same entity."
impl CopilotAgents {
    /// One filter as a palette sub-list.
    fn filter_command(&self, picking: Picking, cx: &Context<Self>) -> PaletteCommand {
        let me = cx.entity().downgrade();
        let options: Vec<PaletteOption> = self
            .filter_options(picking)
            .into_iter()
            .map(|(value, label, checked)| PaletteOption::new(value, label).checked(checked))
            .collect();
        let title = menu_title(picking);
        PaletteCommand::sub(
            format!("{title}\u{2026}"),
            SubList::new(title, options, move |value, _, cx| {
                let value = value.to_string();
                _ = me.update(cx, |this, cx| this.pick_filter(picking, &value, cx));
            }),
        )
        .keywords("copilot agents filter kind state repository cloud local")
        .shortcut(shortcut(picking))
    }

    /// The commands that act on the selected row.
    fn target_commands(&self) -> Vec<PaletteCommand> {
        let Some(row) = self.target() else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        if row.kind == Kind::Local {
            commands.push(
                PaletteCommand::new("Open in chat", OpenInChat)
                    .keywords("copilot pi chat session tab")
                    .shortcut("c"),
            );
            return commands;
        }
        commands.push(
            PaletteCommand::new(
                crate::browser::link(row)
                    .map(|link| link.label)
                    .unwrap_or_default(),
                OpenSession,
            )
            .keywords("browser github agent task page")
            .shortcut("o"),
        );
        if row.pull_request.is_some() {
            commands.push(
                PaletteCommand::new("Open pull request \u{2197}", OpenPullRequest)
                    .keywords("browser github pr")
                    .shortcut("p"),
            );
        }
        commands.push(
            PaletteCommand::new("Copy link", CopyLink)
                .keywords("url clipboard yank")
                .shortcut("y"),
        );
        commands.push(
            PaletteCommand::new(monitor_command(row), ToggleMonitor)
                .keywords("alert watch notify sound toast")
                .shortcut("m"),
        );
        if row.subject.is_some() {
            commands.push(
                PaletteCommand::new("Show the subject", OpenSubject)
                    .keywords("chat subject")
                    .shortcut("u"),
            );
        }
        commands
    }

    /// Whether the loaded task is the one whose log is on show.
    fn log_shown(&self) -> bool {
        match self.log.as_ref().map(|log| log.task_key.as_str()) {
            Some(key) => Some(key) == self.detail_key.as_deref(),
            None => false,
        }
    }

    pub(crate) fn palette_commands(&self, cx: &Context<Self>) -> Vec<PaletteCommand> {
        let mut commands = vec![
            self.filter_command(Picking::Kind, cx),
            self.filter_command(Picking::Group, cx),
            self.filter_command(Picking::Repo, cx),
        ];
        if !self.filters.is_empty() {
            commands.push(
                PaletteCommand::new("Clear the filters", ClearFilters)
                    .keywords("copilot agents filter reset all")
                    .shortcut("0"),
            );
        }
        commands.extend(self.target_commands());
        commands.extend(log_commands(self.log_shown(), cx));
        commands.extend(alert_commands(cx));
        commands
    }
}

/// The alert settings, as the chat's own toggles are offered.
fn alert_commands(cx: &App) -> Vec<PaletteCommand> {
    let settings = alerts::settings(cx);
    let sound = settings.sound;
    let notify = settings.notify;
    vec![
        PaletteCommand::callback(toggle_label("Alert sound", sound), move |_, cx| {
            alerts::set_sound(cx, !sound);
        })
        .keywords("copilot agents alert sound audio chime"),
        PaletteCommand::callback(toggle_label("Alert notification", notify), move |_, cx| {
            alerts::set_notify(cx, !notify);
        })
        .keywords("copilot agents alert notification banner macos"),
        PaletteCommand::sub(
            "Alert sound\u{2026}",
            SubList::new(
                "Play this sound for a task alert\u{2026}",
                SOUNDS
                    .iter()
                    .map(|name| {
                        PaletteOption::new(*name, *name).checked(*name == settings.sound_name)
                    })
                    .collect(),
                |value, _, cx| alerts::set_sound_name(cx, value),
            ),
        )
        .keywords("copilot agents alert sound name glass ping submarine"),
    ]
}
