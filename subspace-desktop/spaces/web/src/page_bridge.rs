//! Between a web board's page and its bridge: requests the page sends, and
//! what the board pushes to it, the app's theme (`subspace:theme`) and the
//! `issues` contract's `changed` (`subspace:issues-changed`).

use gpui_kit::component::Theme;
use gpui_kit::{Context, Subscription};
use subspace_framework::contracts::issues::Changed;
use subspace_framework::dashboard::{ThemeId, contracts};

use crate::bridge::runtime::{self, Incoming};
use crate::{WebBoard, manifest::Permission};

/// What the board follows for its page.
pub(crate) struct Follow {
    /// Whether the board follows the `issues` contract's `changed`.
    issues: bool,
    _theme: [Subscription; 2],
}

impl Follow {
    /// Observes the globals a theme change touches. The push skips a theme
    /// equal to the last one sent.
    pub(crate) fn new(cx: &mut Context<WebBoard>) -> Self {
        Self {
            issues: false,
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

fn issues_changed(board: &WebBoard, cx: &mut Context<WebBoard>) {
    if let (Some(bridge), Some(page), Some(config)) = (
        &board.bridge,
        &board.page,
        runtime::live_config(&board.config.id, cx),
    ) {
        runtime::push_issues_changed(bridge, page, config, cx);
    }
}
