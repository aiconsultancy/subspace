//! One report command and retained form for both product shells, built
//! with the `report-issue` feature. The toolbar entry is a labelled toolbar
//! control; the status bar's is one of its flat commands. Both, and the
//! command, exist only while the app names a repository to file into.

use std::sync::Arc;

use gpui_kit::component::{ActiveTheme as _, WindowExt as _};
use gpui_kit::{App, AppContext as _, Context, Entity, Global, Window};
use subspace_client_github::Client;
use subspace_dashboard::icon_button::IconButtonSize;
use subspace_dashboard::{CurrentProduct, Modal, Toast, services};
use subspace_report_issue::{Destination, IssueRepository, ReportIssue, ReportIssueEvent};

use super::{OpenReportIssue, SubspaceApp};

/// The repository the app's reports file into.
struct ReportTarget(IssueRepository);

impl Global for ReportTarget {}

/// The repository `ProductBuild` names: none when it is missing or blank, an
/// error when it is not `owner/name`.
pub(crate) fn target(repository: Option<&str>) -> Result<Option<IssueRepository>, String> {
    match repository.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some(value) => IssueRepository::parse(value).map(Some).ok_or_else(|| {
            format!("Report issue is off: \"{value}\" is not a GitHub repository (owner/name).")
        }),
    }
}

/// Makes Report issue file into `repository`; without a valid one (see
/// [`target`]) it stays hidden.
pub(crate) fn install(repository: Option<&str>, cx: &mut App) {
    if let Ok(Some(repository)) = target(repository) {
        cx.set_global(ReportTarget(repository));
    }
}

/// Files fixtures' reports into a fictional repository.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn install_fixture_target(cx: &mut App) {
    install(Some("example-org/example-app"), cx);
}

/// Forgets the repository, as in an app that names none.
#[cfg(test)]
pub(crate) fn remove_target(cx: &mut App) {
    cx.remove_global::<ReportTarget>();
}

/// Whether Report issue is offered: the app names a repository.
pub(crate) fn available(cx: &App) -> bool {
    cx.has_global::<ReportTarget>()
}

impl SubspaceApp {
    pub(super) fn open_report_issue(
        &mut self,
        _: &OpenReportIssue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = (available(cx) && self.report_can_open(window, cx))
            .then(|| self.report_form(window, cx));
        match result {
            Some(Ok(form)) => ReportIssue::show(&form, window, cx),
            Some(Err(message)) => self.push_toast(Toast::error(message), cx),
            None => {}
        }
    }

    fn report_can_open(&self, window: &mut Window, cx: &mut App) -> bool {
        !window.has_active_dialog(cx)
            && !window.has_active_sheet(cx)
            && self.settings.is_none()
            && !self.jig_open.get()
            && self.jig.read(cx).can_close()
            && self
                .dashboard()
                .is_none_or(|board| board.modal(cx) != Some(Modal::Dialog))
    }

    fn report_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Entity<ReportIssue>, &'static str> {
        let context = cx.global::<CurrentProduct>().0.clone();
        let diagnostics = crate::diagnostics::issue::collect(
            &context,
            self.dashboard().map(|board| board.id),
            cx.theme().theme_name(),
            IconButtonSize::current(cx).key(),
        );
        let paths = self.report_path_diagnostics(cx);
        if let Some(form) = &self.report_issue {
            form.update(cx, |form, cx| {
                form.set_path_diagnostics(paths, cx);
                form.set_diagnostics(diagnostics, cx);
            });
            return Ok(form.clone());
        }
        const UNAVAILABLE: &str = "Cannot open Report issue. Restart the app and try again.";
        let credentials = services(cx).ok_or(UNAVAILABLE)?.credentials.clone();
        let repository = cx
            .try_global::<ReportTarget>()
            .ok_or(UNAVAILABLE)?
            .0
            .clone();
        let destination = Destination {
            client: Arc::new(Client::new(credentials)),
            repository,
        };
        let form = cx.new(|cx| {
            let mut form = ReportIssue::new(
                destination,
                diagnostics,
                context.env.home.clone(),
                window,
                cx,
            );
            form.set_path_diagnostics(paths, cx);
            form
        });
        self._subscriptions
            .push(cx.subscribe(&form, |app, _, event, cx| match event {
                ReportIssueEvent::Toast(toast) => app.push_toast(toast.clone(), cx),
            }));
        self.report_issue = Some(form.clone());
        Ok(form)
    }

    fn report_path_diagnostics(&self, cx: &App) -> subspace_core::diagnostics::IssuePaths {
        let path = self
            .dashboard()
            .and_then(|board| board.diagnostic_config_path(cx))
            .map(|path| path.to_string_lossy().into_owned());
        subspace_dashboard::copy_details::issue_paths(path.as_deref(), cx)
    }
}

#[cfg(test)]
mod tests {
    use super::target;

    #[test]
    fn a_missing_or_blank_repository_is_none_and_a_malformed_one_is_refused() {
        assert_eq!(target(None), Ok(None));
        assert_eq!(target(Some("  ")), Ok(None));
        let repository = target(Some("example-org/example-app")).unwrap().unwrap();
        assert_eq!(repository.as_str(), "example-org/example-app");
        let refused = target(Some("example-app")).unwrap_err();
        assert!(refused.contains("\"example-app\""), "{refused}");
    }
}
