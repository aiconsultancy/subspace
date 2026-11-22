//! Between a web board's page and its bridge: requests the page sends, and
//! what the board pushes to it, the app's theme (`subspace:theme`) and the
//! `issues` contract's `changed` (`subspace:issues-changed`), and for the
//! built-in Fleet Console a change to the app's fleets or their tokens
//! (`subspace:fleets-changed`).

use gpui_kit::component::Theme;
use gpui_kit::{Context, Subscription};
use subspace_client_fleet::FleetApi;
use subspace_framework::contracts::issues::Changed;
use subspace_framework::dashboard::{ThemeId, contracts};

use crate::bridge::runtime::{self, Incoming};
use crate::{WebBoard, manifest::Permission};

/// What the board follows for its page.
pub(crate) struct Follow {
    /// Whether the board follows the `issues` contract's `changed`.
    issues: bool,
    /// The app's fleet service, followed for the built-in Fleet Console,
    /// with the fleet list's revision last pushed.
    fleets: Option<(Subscription, u64)>,
    _theme: [Subscription; 2],
}

impl Follow {
    /// Observes the globals a theme change touches. The push skips a theme
    /// equal to the last one sent.
    pub(crate) fn new(cx: &mut Context<WebBoard>) -> Self {
        Self {
            issues: false,
            fleets: None,
            _theme: [
                cx.observe_global::<Theme>(theme_changed),
                cx.observe_global::<ThemeId>(theme_changed),
            ],
        }
    }
}

/// Answers one request from the page, with the live config's grants.
pub(crate) fn receive(board: &mut WebBoard, message: Incoming<'_>, cx: &mut Context<WebBoard>) {
    if let (Some(page), Some(bridge), Some(config)) = (
        &board.page,
        &mut board.bridge,
        runtime::live_config(&board.config.id, cx).cloned(),
    ) {
        runtime::receive(bridge, page, &config, message, cx);
    }
}

fn theme_changed(board: &mut WebBoard, cx: &mut Context<WebBoard>) {
    if let (Some(bridge), Some(page)) = (&mut board.bridge, &board.page) {
        runtime::push_theme(bridge, page, cx);
    }
}

/// Follows the `issues` contract for an app whose manifest asks for it;
/// the `issues` grant decides each time whether the page hears.
pub(crate) fn attach_issues(board: &mut WebBoard, cx: &mut Context<WebBoard>) {
    let Some(bridge) = &board.bridge else { return };
    if board.follow.issues || !bridge.requests(Permission::Issues) {
        return;
    }
    let entity = cx.entity();
    let handler =
        |board: &mut WebBoard, _: &Changed, cx: &mut Context<WebBoard>| issues_changed(board, cx);
    board.follow.issues = contracts::subscribe(bridge.board, &entity, handler, cx).is_ok();
}

/// Follows the app's fleets for the built-in Fleet Console, so its page
/// hears when a fleet, its address or its token changes.
pub(crate) fn attach_fleets(board: &mut WebBoard, cx: &mut Context<WebBoard>) {
    let Some(bridge) = &board.bridge else { return };
    if board.follow.fleets.is_some() || !bridge.shares_fleets() {
        return;
    }
    let service = FleetApi::shared(cx);
    let seen = service.read(cx).directory_revision();
    let subscription = cx.observe(&service, |board: &mut WebBoard, service, cx| {
        let revision = service.read(cx).directory_revision();
        if let Some((_, seen)) = &mut board.follow.fleets
            && std::mem::replace(seen, revision) != revision
        {
            fleets_changed(board, cx);
        }
    });
    board.follow.fleets = Some((subscription, seen));
}

fn fleets_changed(board: &WebBoard, cx: &mut Context<WebBoard>) {
    if let (Some(bridge), Some(page), Some(config)) = (
        &board.bridge,
        &board.page,
        runtime::live_config(&board.config.id, cx),
    ) {
        runtime::push_fleets_changed(bridge, page, config, cx);
    }
}

fn issues_changed(board: &WebBoard, cx: &mut Context<WebBoard>) {
    if let (Some(bridge), Some(page), Some(config)) = (
        &board.bridge,
        &board.page,
        runtime::live_config(&board.config.id, cx),
    ) {
        runtime::push_issues_changed(bridge, page, config, cx);
    }
}
