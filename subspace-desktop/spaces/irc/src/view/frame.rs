//! The Space in the shared workspace shell: the buffer list in the
//! navigator, the conversations' dock area in the pane under its toolbar,
//! and the focused channel's members in the inspector. The docks' switches
//! are the status bar's (`Dashboard::docks`).

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, KeyContext, ParentElement as _,
    Render, Styled as _, Window, div,
};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{
    DASHBOARD_CONTEXT, Dashboard as _, Tokens, list, measure, workspace,
};

use super::{bind_hub, conversations, empty};
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
            frame = frame.inspector(super::inspector::inspector(self, cx));
        }
        // The root carries the Space's key context and focus, so the board
        // stays focusable and its keys answer while the navigator, whose
        // list also tracks them, is hidden.
        let mut context = KeyContext::new_with_defaults(cx.platform_info().os);
        context.add(DASHBOARD_CONTEXT);
        context.add(Self::ID);
        v_flex()
            .key_context(context)
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .child(frame)
            .children(self.initiative_menu.host())
            .children(join::render(self, cx))
    }
}

/// Whether there is a network to show: until then the pane says so, the
/// navigator stays and there is no inspector. Never while Channels is closed
/// to someone who belongs to no initiative.
fn configured(board: &Irc, cx: &App) -> bool {
    board.hub.as_ref().is_some_and(|hub| {
        let hub = hub.read(cx);
        !hub.initiatives.closed()
            && !hub.initiatives.awaiting_read()
            && !hub.state.networks().is_empty()
    })
}

/// Whether the networks wait for the first read of the memberships.
fn awaiting_read(board: &Irc, cx: &App) -> bool {
    board
        .hub
        .as_ref()
        .is_some_and(|hub| hub.read(cx).initiatives.awaiting_read())
}

/// Whether the API that brings the person's networks cannot be reached,
/// where initiatives carry channels.
fn unreachable(board: &Irc, cx: &App) -> bool {
    subspace_api_client::capabilities().initiative_channels
        && board
            .hub
            .as_ref()
            .is_some_and(|hub| hub.read(cx).api_unreachable(cx))
}

/// Whether Channels is closed until the person belongs to an initiative.
pub(crate) fn closed(board: &Irc, cx: &App) -> bool {
    board
        .hub
        .as_ref()
        .is_some_and(|hub| hub.read(cx).initiatives.closed())
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
/// before it has a network to show or while it is closed.
fn pane(board: &mut Irc, configured: bool, cx: &mut Context<Irc>) -> workspace::Pane {
    let pane = workspace::Pane::new("irc-pane");
    let area = match &board.hub {
        None => return pane.child(empty::stopped(cx)),
        Some(_) if closed(board, cx) => return pane.child(empty::not_a_member(board, cx)),
        Some(_) if awaiting_read(board, cx) => return pane.child(empty::checking(cx)),
        Some(_) if !configured && unreachable(board, cx) => {
            return pane.child(empty::unreachable(board, cx));
        }
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
    if let Some(actions) = toolbar::render(board, cx) {
        bar = bar.trailing(actions);
    }
    bar.trailing(
        subspace_framework::dashboard::action::icon_button(
            "irc-refresh",
            subspace_framework::dashboard::action::Intent::Quiet,
            subspace_framework::lucide::IconName::RefreshCw,
            "Refresh",
        )
        .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))),
    )
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
