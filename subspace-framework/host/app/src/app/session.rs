//! The palette session: what the palette shows, and what a confirmed line
//! does.

use std::rc::Rc;

use gpui_kit::component::IndexPath;
use gpui_kit::{Action, Context, SharedString, Window};
use subspace_dashboard::{CommandRun, Modal, PaletteCommand, PaletteMode, SubList};

use super::{
    MainSurface, PaletteBackspace, PaletteEscape, SearchItems, SubspaceApp, TogglePalette,
};
use crate::palette::{self, Entry, Section};

mod commands;

pub(super) struct PaletteSession {
    pub(super) mode: PaletteMode,
    pub(super) sub: Option<Rc<SubList>>,
    /// What the palette shows, so a confirmed index finds its entry.
    pub(super) sections: Vec<Section>,
    /// The sub-list option to highlight once the palette shows the sub-list.
    pub(super) pending_select: Option<(usize, u8)>,
    /// The palette state holds the sub-list's entries (the previous render
    /// built them).
    pub(super) sub_installed: bool,
}

enum Run {
    Sub(Rc<SubList>),
    Action(Box<dyn Action>),
    Callback(subspace_dashboard::Callback),
    Item(SharedString),
    Pick(SharedString),
}

fn run_of(entry: &Entry) -> Run {
    match entry {
        Entry::Command(command) => match &command.run {
            CommandRun::Sub(sub) => Run::Sub(sub.clone()),
            CommandRun::Action(action) => Run::Action(action.boxed_clone()),
            CommandRun::Callback(callback) => Run::Callback(callback.clone()),
        },
        Entry::Item(id, _) => Run::Item(id.clone()),
        Entry::Option(option) => Run::Pick(option.value.clone()),
    }
}

impl SubspaceApp {
    pub(super) fn open_palette(
        &mut self,
        mode: PaletteMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.is_some() || super::sign_in_gate::shown(cx) {
            return;
        }
        self.dismiss_shortcuts(window, cx);
        if let Some(dashboard) = self.dashboards.get(self.active) {
            if dashboard.modal(cx) == Some(Modal::Dialog) {
                return;
            }
            dashboard.dismiss(window, cx);
        }
        self.help_open = false;
        self.session = Some(PaletteSession {
            mode,
            sub: None,
            sections: Vec::new(),
            pending_select: None,
            sub_installed: false,
        });
        self.reset_query(window, cx);
        cx.notify();
    }

    pub(super) fn reset_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette.update(cx, |state, cx| {
            state.set_query("", window, cx);
            state.focus(window, cx);
        });
    }

    pub(super) fn toggle_palette(
        &mut self,
        _: &TogglePalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.is_some() {
            self.close_palette(window, cx);
        } else {
            self.open_palette(PaletteMode::Commands, window, cx);
        }
    }

    pub(super) fn search_items(
        &mut self,
        _: &SearchItems,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dashboard().and_then(|d| d.search(cx)).is_some() {
            self.open_palette(PaletteMode::Items, window, cx);
        }
    }

    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session = None;
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn set_sub(
        &mut self,
        sub: Option<Rc<SubList>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = &mut self.session {
            session.pending_select = sub.as_deref().map(|sub| (palette::checked_index(sub), 0));
            session.sub_installed = false;
            session.sub = sub;
        }
        self.reset_query(window, cx);
        cx.notify();
    }

    pub(super) fn palette_escape(
        &mut self,
        _: &PaletteEscape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.in_sub_list() {
            self.set_sub(None, window, cx);
        } else {
            self.close_palette(window, cx);
        }
    }

    pub(super) fn palette_backspace(
        &mut self,
        _: &PaletteBackspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.in_sub_list() && self.palette.read(cx).query(cx).is_empty() {
            self.set_sub(None, window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn in_sub_list(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.sub.is_some())
    }

    pub(super) fn shell_commands(&self, cx: &mut Context<Self>) -> Vec<PaletteCommand> {
        let wrap = subspace_dashboard::wrap(cx);
        let me = cx.weak_entity();
        let mut commands: Vec<PaletteCommand> = self
            .dashboards
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.active)
            .map(|(index, dashboard)| {
                let me = me.clone();
                PaletteCommand::callback(format!("Go to {}", dashboard.title), move |window, cx| {
                    _ = me.update(cx, |app, cx| app.switch_to(index, window, cx));
                })
                .keywords("view switch open dashboard")
            })
            .collect();
        commands.push(super::add_space::palette_command(cx));
        if self.main.is_board()
            && !self.help_zoomed(cx)
            && let Some(board) = self.dashboard()
        {
            commands.extend(board.find_commands(cx));
        }
        commands.extend(commands::global(
            wrap,
            self.jig_open.get(),
            self.main.is_logs(),
        ));
        if let Some(dock) = &self.terminal {
            commands.extend(commands::terminal(
                dock.state.open,
                dock.state.layout.maximized,
            ));
        }
        #[cfg(debug_assertions)]
        commands.extend(crate::inspector::palette_commands(cx));
        commands
    }

    fn entry_run(&self, index: IndexPath) -> Option<Run> {
        let session = self.session.as_ref()?;
        session
            .sections
            .get(index.section)
            .and_then(|section| section.entries.get(index.row))
            .map(run_of)
    }

    /// What the recent commands store for the command at `index`.
    fn entry_command_key(&self, index: IndexPath) -> Option<String> {
        let entry = self
            .session
            .as_ref()?
            .sections
            .get(index.section)?
            .entries
            .get(index.row)?;
        match entry {
            Entry::Command(command) => Some(crate::palette::command_recent_key(command).to_owned()),
            Entry::Item(_, _) | Entry::Option(_) => None,
        }
    }

    pub(super) fn run_entry(
        &mut self,
        index: IndexPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sub = self.session.as_ref().and_then(|s| s.sub.clone());
        let command_key = self.entry_command_key(index);
        let Some(run) = self.entry_run(index) else {
            return;
        };
        if let Run::Sub(sub) = run {
            self.set_sub(Some(sub), window, cx);
            return;
        }
        if let Some(key) = command_key {
            commands::remember(key, cx);
        }
        self.close_palette(window, cx);
        self.dispatch_run(run, sub, window, cx);
    }

    fn dispatch_run(
        &mut self,
        run: Run,
        sub: Option<Rc<SubList>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match run {
            Run::Sub(_) => {}
            Run::Action(action) => {
                window.defer(cx, move |window, cx| window.dispatch_action(action, cx));
            }
            Run::Callback(callback) => window.defer(cx, move |window, cx| callback(window, cx)),
            Run::Item(id) => self.open_item(&id, window, cx),
            Run::Pick(value) => {
                if let Some(sub) = sub {
                    window.defer(cx, move |window, cx| (sub.on_pick)(&value, window, cx));
                }
            }
        }
    }

    pub(super) fn open_item(
        &mut self,
        id: &SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dashboard_id) = self.dashboard().map(|dashboard| dashboard.id) else {
            return;
        };
        self.push_recent(dashboard_id, id.to_string(), cx);
        // Returning from Search or Logs wakes the board before it opens the item.
        self.set_main(MainSurface::Board, window, cx);
        self.search_navigation = self.search_navigation.wrapping_add(1);
        if let Some(dashboard) = self.dashboard() {
            dashboard.open_search_result(id, window, cx);
        }
    }
}
