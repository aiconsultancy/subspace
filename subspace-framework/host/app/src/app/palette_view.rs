//! The command palette overlay: its entries, its header and its lines.

use gpui_kit::component::IndexPath;
use gpui_kit::component::command::Command;
use gpui_kit::{
    AnyElement, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div, px,
};
use subspace_dashboard::{PaletteMode, Search, SubList, Tokens};

use super::SubspaceApp;
use super::keys::PALETTE_CONTEXT;
use super::palette_lines::{group_of, header_element};
use super::shell::overlay;
use crate::palette::{self, Section};

/// What the palette must show this frame.
struct PaletteView {
    sections: Vec<Section>,
    query: String,
    placeholder: SharedString,
    empty_text: SharedString,
    subject: Option<subspace_dashboard::Row>,
    in_sub: bool,
}

/// Palette layout, in pixels.
const PANEL_MAX_HEIGHT: f32 = 420.;
const PANEL_WIDTH: f32 = 640.;
const PANEL_RADIUS: f32 = 8.;
/// How often the palette retries selecting a sub-list's checked option
/// before it gives up; the query reset can undo the selection.
pub const MAX_SELECT_TRIES: u8 = 5;

impl SubspaceApp {
    /// A sub-list opens at its checked option, once the palette state has
    /// the sub-list's entries. The query reset can clear the selection after
    /// it, so keep trying until the palette reports the option as selected.
    pub(super) fn apply_pending_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        if !session.sub_installed {
            return;
        }
        let Some((row, tries)) = session.pending_select else {
            return;
        };
        let target = IndexPath::new(row);
        let selected = self.palette.read(cx).selected_index() == Some(target);
        let Some(next) = keep_trying(selected, tries) else {
            if let Some(session) = &mut self.session {
                session.pending_select = None;
            }
            return;
        };
        if let Some(session) = &mut self.session {
            session.pending_select = Some((row, next));
        }
        self.palette.update(cx, |state, cx| {
            state.set_selected_index(Some(target), window, cx);
        });
        cx.on_next_frame(window, |_, _, cx| cx.notify());
    }

    /// What the palette must show this frame.
    fn palette_view(&mut self, cx: &mut Context<Self>) -> Option<PaletteView> {
        let session = self.session.as_ref()?;
        let (mode, sub) = (session.mode, session.sub.clone());
        let dashboard = self.dashboards.get(self.active)?;
        let dashboard_id = dashboard.id;
        let mut commands = dashboard.commands(cx);
        let search = dashboard.search(cx);
        let subject = (mode == PaletteMode::Commands && sub.is_none())
            .then(|| dashboard.subject(cx))
            .flatten();
        commands.extend(self.shell_commands(cx));

        let query = self.palette.read(cx).query(cx).to_string();
        let recent = self.recent.get(dashboard_id).cloned().unwrap_or_default();
        let recent_commands = subspace_dashboard::prefs::get_list(cx, palette::RECENT_COMMANDS_KEY);
        let sections = palette::sections(&palette::Request {
            query: &query,
            mode,
            sub: sub.as_deref(),
            commands: &commands,
            recent_commands: &recent_commands,
            search: search.as_ref(),
            recent: &recent,
        });
        let search_label = search
            .as_ref()
            .map_or(SharedString::from("Search"), |s| s.placeholder.clone());
        Some(PaletteView {
            placeholder: placeholder(sub.as_deref(), mode, search.as_ref()),
            empty_text: empty_text(&query, &search_label),
            in_sub: sub.is_some(),
            subject,
            sections,
            query,
        })
    }

    pub(super) fn render_palette(
        &mut self,
        tokens: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.apply_pending_selection(window, cx);
        let view = self.palette_view(cx)?;
        let command = self.build_command(&view, tokens);
        self.install_sections(view.sections, window, cx);
        Some(palette_overlay(command, cx))
    }

    /// The palette widget for `view`: its field, its groups, its empty text
    /// and its header.
    fn build_command(&self, view: &PaletteView, tokens: &Tokens) -> Command {
        let command = Command::new(&self.palette)
            .placeholder(view.placeholder.clone())
            .filterable(false)
            .max_h(px(PANEL_MAX_HEIGHT))
            .w(px(PANEL_WIDTH))
            .bg(tokens.surface);
        let command = with_groups(command, view, tokens);
        let command = with_empty(command, view.empty_text.clone(), tokens.muted);
        with_header(command, view, tokens)
    }

    /// Keep the shown sections, so a confirmed index finds its entry.
    pub(super) fn install_sections(
        &mut self,
        sections: Vec<Section>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = &mut self.session else {
            return;
        };
        session.sections = sections;
        if session.sub.is_some() && session.pending_select.is_some() {
            session.sub_installed = true;
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }
    }
}

/// The palette's backdrop and its confirm, cancel and escape wiring.
fn palette_overlay(command: Command, cx: &mut Context<SubspaceApp>) -> AnyElement {
    let me = cx.weak_entity();
    let command = command
        .on_query(|_, window, _| window.refresh())
        .on_confirm({
            let me = me.clone();
            move |index, window, cx| {
                _ = me.update(cx, |app, cx| app.run_entry(index, window, cx));
            }
        })
        .on_cancel(move |window, cx| {
            _ = me.update(cx, |app, cx| app.close_palette(window, cx));
        });
    overlay()
        .id("palette-backdrop")
        .key_context(PALETTE_CONTEXT)
        .on_action(cx.listener(SubspaceApp::palette_escape))
        .on_action(cx.listener(SubspaceApp::palette_backspace))
        .on_click(cx.listener(|app, _, window, cx| app.close_palette(window, cx)))
        .child(
            div()
                .id("palette")
                .rounded(px(PANEL_RADIUS))
                .shadow_lg()
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(command),
        )
        .into_any_element()
}

/// One group per palette section, in order.
fn with_groups(mut command: Command, view: &PaletteView, tokens: &Tokens) -> Command {
    for section in &view.sections {
        command = command.group(group_of(section, &view.query, tokens));
    }
    command
}

/// What the palette shows when it lists nothing.
fn with_empty(command: Command, text: SharedString, muted: Hsla) -> Command {
    command.empty(move |_, _, _| div().px_3().py_2().text_color(muted).child(text.clone()))
}

/// The subject row, or the way back from a sub-list.
fn with_header(command: Command, view: &PaletteView, tokens: &Tokens) -> Command {
    let (in_sub, subject) = (view.in_sub, view.subject.clone());
    if !in_sub && subject.is_none() {
        return command;
    }
    let tokens = *tokens;
    command
        .header(move |_, _, _| header_element(subject.clone(), in_sub, &tokens).unwrap_or_else(div))
}

/// The next try count, or `None` once the option is selected or the tries
/// run out.
fn keep_trying(selected: bool, tries: u8) -> Option<u8> {
    (!selected && tries < MAX_SELECT_TRIES).then_some(tries + 1)
}

/// What the palette's field invites, by mode.
fn placeholder(sub: Option<&SubList>, mode: PaletteMode, search: Option<&Search>) -> SharedString {
    match (sub, mode) {
        (Some(sub), _) => sub.placeholder.clone(),
        (None, PaletteMode::Items) => search.map_or("Search".into(), |s| s.placeholder.clone()),
        (None, PaletteMode::Commands) => "Type a command or search items…".into(),
    }
}

/// What the palette says when it lists nothing.
fn empty_text(query: &str, search_label: &str) -> SharedString {
    if query.trim().is_empty() {
        format!("Type to {}", search_label.to_lowercase()).into()
    } else {
        "No match".into()
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SELECT_TRIES, empty_text, keep_trying};

    #[test]
    pub(super) fn the_checked_option_is_retried_until_it_sticks() {
        assert_eq!(keep_trying(false, 0), Some(1));
        assert_eq!(
            keep_trying(false, MAX_SELECT_TRIES - 1),
            Some(MAX_SELECT_TRIES)
        );
        assert_eq!(keep_trying(false, MAX_SELECT_TRIES), None);
        assert_eq!(keep_trying(true, 0), None);
    }

    #[test]
    pub(super) fn an_empty_palette_invites_the_dashboard_search() {
        assert_eq!(
            empty_text("  ", "Search this board"),
            "Type to search this board"
        );
        assert_eq!(empty_text("zz", "Search this board"), "No match");
    }
}
