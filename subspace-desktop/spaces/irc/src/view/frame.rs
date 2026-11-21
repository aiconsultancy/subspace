//! The Space in the shared workspace shell: the buffer list in the
//! navigator, the conversations' dock area in the pane under its toolbar,
//! and the focused channel's members in the inspector. The docks' switches
//! are the status bar's (`Dashboard::docks`).

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, IntoElement, ParentElement as _, Render, Styled as _, Window, div,
};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{Dashboard as _, Tokens, feedback, list, measure, workspace};

use super::{bind_hub, conversations, details, empty};
use crate::{Irc, dock, initiatives, join, toolbar};

impl Render for Irc {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        bind_hub(self, cx);
        self.text_edge = workspace::text_edge(&Tokens::current(cx));
        let configured = configured(self, cx);
        let centre = pane(self, configured, cx);
        let mut frame = workspace::Workspace::new("irc-workspace").pane(centre);
        // Before there is a network the status bar has no switch to bring
        // the navigator back with, and the navigator's title names the Space.
        if self.dock.buffers_open() || !configured {
            frame = frame.navigator(navigator(self, configured, window, cx));
        }
        if configured && self.dock.members_open() {
            frame = frame.inspector(inspector(self, cx));
        }
        v_flex()
            .relative()
            .size_full()
            .child(frame)
            .children(self.initiative_menu.host())
            .children(join::render(self, cx))
    }
}

/// Whether there is a network to show: until then the pane says so, the
/// navigator stays and there is no inspector.
fn configured(board: &Irc, cx: &App) -> bool {
    board
        .hub
        .as_ref()
        .is_some_and(|hub| !hub.read(cx).state.networks().is_empty())
}

/// The docks the status bar's switches show: both, once there is a network
/// to show; none before.
pub(crate) fn space_docks(board: &Irc, cx: &App) -> SpaceDocks {
    if !configured(board, cx) {
        return SpaceDocks::NONE;
    }
    SpaceDocks::both(board.dock.buffers_open(), board.dock.members_open())
}

/// A dock's status bar switch was used: the buffer list or the members.
pub(crate) fn toggle_space_dock(board: &mut Irc, which: SpaceDock, cx: &mut Context<Irc>) {
    match which {
        SpaceDock::Navigator => dock::toggle_buffers(board, cx),
        SpaceDock::Inspector => dock::toggle_members(board, cx),
    }
}

/// The navigator: the Space's name, Join, and the networks with their
/// buffers. The shared list draws them, so the rows keep their selection,
/// keys, folding and menus.
fn navigator(
    board: &Irc,
    configured: bool,
    window: &mut Window,
    cx: &mut Context<Irc>,
) -> workspace::Navigator {
    let navigator = workspace::Navigator::new(Irc::TITLE);
    if !configured {
        return navigator;
    }
    let tokens = Tokens::current(cx);
    let buffers = list::render(board, window, cx)
        .size_full()
        .bg(tokens.sidebar);
    navigator
        .action(toolbar::navigator_join(board, cx))
        .child(buffers)
}

/// The pane: the toolbar over the conversations, or what the Space says
/// before it has a network to show.
fn pane(board: &mut Irc, configured: bool, cx: &mut Context<Irc>) -> workspace::Pane {
    let pane = workspace::Pane::new("irc-pane");
    let area = match &board.hub {
        None => return pane.child(empty::stopped(cx)),
        Some(_) if !configured => return pane.child(empty::unconfigured(board, cx)),
        Some(_) => board.dock.area.clone(),
    };
    let content = div()
        .relative()
        .flex_1()
        .min_h_0()
        .w_full()
        .child(area)
        .child(measure::height(board.available_height.clone()));
    pane.toolbar(pane_toolbar(board, cx)).child(content)
}

/// The pane's toolbar: the conversation in front on the leading side, the
/// network and channel actions on the trailing side. While the navigator is
/// hidden a menu of the conversations leads it, and names the one in front.
fn pane_toolbar(board: &mut Irc, cx: &mut Context<Irc>) -> workspace::Toolbar {
    let navigator_open = board.dock.buffers_open();
    let mut bar = workspace::Toolbar::new("irc-toolbar");
    if !navigator_open {
        bar = bar.leading(conversations::button(board));
    }
    if let Some(picker) = initiatives::button(board, cx) {
        bar = bar.leading(picker);
    }
    if let Some(heading) = heading(board, navigator_open, cx) {
        bar = bar.leading(heading);
    }
    match toolbar::render(board, cx) {
        Some(actions) => bar.trailing(actions),
        None => bar,
    }
}

/// The conversation in front as the toolbar's heading: its name, unless a
/// menu beside it says it, then its network and topic. It takes the room
/// the leading side has left and never wraps under the picker.
fn heading(board: &Irc, named: bool, cx: &App) -> Option<AnyElement> {
    let focused = board.dock.focused.as_ref()?;
    let conversation = board.dock.open.get(focused)?.entity.read(cx);
    let heading = conversation.heading(named, cx)?;
    Some(div().flex_1().min_w_0().child(heading).into_any_element())
}

/// The inspector: the focused channel's members under their count, or a
/// hint for a buffer that has none.
fn inspector(board: &Irc, cx: &mut Context<Irc>) -> workspace::Inspector {
    let members = dock::focused_members(board);
    let count = members.as_ref().map(|list| list.read(cx).count(cx));
    let body = match members {
        Some(list) => list.into_any_element(),
        None => no_channel(cx),
    };
    workspace::Inspector::new("irc-inspector", details::members_title(count))
        .on_close(
            "Hide members",
            cx.listener(|board, _, _, cx| dock::toggle_members(board, cx)),
        )
        .child(body)
}

/// What the inspector says for the server buffer and private conversations.
fn no_channel(cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let hint = "Open a channel to see its members.";
    feedback::empty(&tokens, "irc-members-none", "No channel", hint).into_any_element()
}
