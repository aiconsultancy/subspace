//! Actions only some builds have: Report issue, with the `report-issue`
//! feature, and the developer settings, in debug builds.

use gpui_kit::{Context, Div, InteractiveElement as _, Stateful};

use super::SubspaceApp;

impl SubspaceApp {
    #[cfg(feature = "report-issue")]
    pub(super) fn report_issue_actions(
        shell: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        shell.on_action(cx.listener(Self::open_report_issue))
    }

    #[cfg(not(feature = "report-issue"))]
    pub(super) fn report_issue_actions(
        shell: Stateful<Div>,
        _: &mut Context<Self>,
    ) -> Stateful<Div> {
        shell
    }

    #[cfg(debug_assertions)]
    pub(super) fn developer_actions(shell: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        shell.on_action(cx.listener(Self::open_developer_settings))
    }

    #[cfg(not(debug_assertions))]
    pub(super) fn developer_actions(shell: Stateful<Div>, _: &mut Context<Self>) -> Stateful<Div> {
        shell
    }
}
