use gpui_kit::{AppContext as _, ClipboardItem, Context, Focusable as _, Window};
use subspace_client_github::issue_report::{
    self, BrowserMode, Error, ErrorKind, IssueRepository, Outcome, ReportPlan,
};
use subspace_dashboard::Toast;

use crate::{ReportIssue, ReportIssueEvent, report::Phase};

// qual:allow(coupling, oi) reason: "False positive: background submission methods belong to the same retained ReportIssue owner in report.rs; the module split does not cross a feature or ownership boundary."
impl ReportIssue {
    pub(super) fn review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.busy() || !matches!(self.state.phase, Phase::Editing) {
            return;
        }
        let draft = self.form.draft(cx);
        let report =
            match issue_report::prepare(&draft, &self.effective_diagnostics(), &self.repository) {
                Ok(report) => report,
                Err(error) => {
                    self.state.error = Some(error.to_string());
                    cx.notify();
                    return;
                }
            };
        self.state.phase = Phase::Preparing;
        self.state.error = None;
        let revision = self.state.revision;
        let client = self.client.clone();
        let task = cx.background_spawn(async move { client.prepare_issue_report(report) });
        cx.spawn_in(window, async move |this, cx| {
            let plan = task.await;
            _ = this.update_in(cx, |this, window, cx| {
                this.prepared(plan, revision, window, cx)
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn prepared(
        &mut self,
        plan: ReportPlan,
        revision: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.revision != revision || !matches!(self.state.phase, Phase::Preparing) {
            return;
        }
        self.form.set_preview(plan.title(), plan.body(), window, cx);
        self.state.phase = Phase::Review(Box::new(plan));
        if self.state.open {
            self.form.preview_title.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.busy() || !matches!(self.state.phase, Phase::Review(_)) {
            return;
        }
        let Phase::Review(plan) = std::mem::replace(&mut self.state.phase, Phase::Submitting)
        else {
            return;
        };
        let client = self.client.clone();
        let task = cx.background_spawn(async move { client.submit_issue_report(*plan) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update_in(cx, |this, _, cx| this.submitted(result, cx));
        })
        .detach();
        cx.notify();
    }

    pub(super) fn submitted(&mut self, result: Result<Outcome, Error>, cx: &mut Context<Self>) {
        match result {
            Ok(Outcome::Created(issue)) => {
                let url = issue.url().to_owned();
                // Commit the verified URL before any external side effect.
                self.state.phase = Phase::Created(url.clone());
                self.state.body_pending = false;
                cx.emit(ReportIssueEvent::Toast(
                    Toast::info("Issue created").link(url.clone(), "View issue"),
                ));
                cx.open_url(&url);
            }
            Ok(Outcome::Browser(fallback)) => {
                self.open_browser_fallback(fallback, cx);
            }
            Err(error) => self.submission_failed(error),
        }
        cx.notify();
    }

    fn open_browser_fallback(
        &mut self,
        fallback: issue_report::BrowserFallback,
        cx: &mut Context<Self>,
    ) {
        let url = fallback.url().to_owned();
        let body = fallback.clipboard_body().map(str::to_owned);
        self.state.phase = Phase::Browser(fallback);
        self.state.body_pending = body.is_some();
        if let Some(body) = body {
            cx.write_to_clipboard(ClipboardItem::new_string(body));
            self.state.notice = Some("Body sent to the clipboard. Paste it into GitHub first, then choose I pasted the body to enable Copy image. Submit the issue in GitHub.".into());
        } else {
            self.state.notice = Some("GitHub's editor was requested. Review and submit the issue there. No issue creation is confirmed.".into());
        }
        cx.open_url(&url);
    }

    fn submission_failed(&mut self, error: Error) {
        self.state.phase = if error.kind() == ErrorKind::UncertainCreation {
            Phase::Uncertain { opened: false }
        } else {
            Phase::Editing
        };
        self.state.error = Some(error.to_string());
    }

    pub(super) fn copy_body(&mut self, cx: &mut Context<Self>) {
        if self.state.busy() {
            return;
        }
        let Phase::Browser(fallback) = &self.state.phase else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(fallback.body().to_owned()));
        self.state.body_pending = true;
        self.state.notice = Some("Body sent to the clipboard. Paste it into GitHub's description, then choose I pasted the body to enable Copy image. Copy title replaces it; Copy body restores it.".into());
        cx.notify();
    }

    pub(super) fn copy_title(&mut self, cx: &mut Context<Self>) {
        if self.state.busy() || !matches!(self.state.phase, Phase::Browser(_)) {
            return;
        }
        let title = self.form.preview_title.read(cx).value().to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(title));
        self.state.notice = Some("Title sent to the clipboard. Paste it into GitHub's title field. Copy body or Copy image replaces it; Copy title restores it.".into());
        cx.notify();
    }

    pub(super) fn acknowledge_body(&mut self, cx: &mut Context<Self>) {
        if self.state.busy()
            || !self.state.body_pending
            || !matches!(self.state.phase, Phase::Browser(_))
        {
            return;
        }
        self.state.body_pending = false;
        self.state.notice =
            Some("Body marked as pasted. Copy and paste each image in order.".into());
        cx.notify();
    }

    pub(super) fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.busy() || !matches!(self.state.phase, Phase::Review(_)) {
            return;
        }
        self.state.phase = Phase::Editing;
        self.state.invalidate();
        self.form.description.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(super) fn check_github(&mut self, cx: &mut Context<Self>) {
        if let Phase::Uncertain { opened } = &mut self.state.phase {
            *opened = true;
            cx.open_url(&self.repository.issues_url());
            cx.notify();
        }
    }

    pub(super) fn retry_checked(&mut self, cx: &mut Context<Self>) {
        if matches!(self.state.phase, Phase::Uncertain { opened: true }) {
            self.state.phase = Phase::Editing;
            self.state.error = None;
            cx.notify();
        }
    }
}

pub(super) fn submit_label(phase: &Phase) -> &'static str {
    match phase {
        Phase::Review(plan) if plan.transport() == issue_report::TransportKind::Browser => {
            "Open GitHub editor"
        }
        _ => "Create GitHub issue",
    }
}

pub(super) fn review_instruction(phase: &Phase, repository: &IssueRepository) -> String {
    let instruction = match phase {
        Phase::Review(plan)
            if plan.transport() == issue_report::TransportKind::Browser
                && plan.browser_fallback().editor_mode() == BrowserMode::ClipboardRequired =>
        {
            "The report is too long for a browser URL. The exact full body below will be sent to the clipboard first. Paste it into GitHub, then use Copy image separately."
        }
        Phase::Review(plan) if plan.transport() == issue_report::TransportKind::Browser => {
            "No GitHub credential is available. Open the prefilled editor and submit the issue there. The exact effective text is shown below."
        }
        _ => {
            return format!(
                "Create one issue in {}. Images are not uploaded. Copy and paste each image into the issue in GitHub's browser editor.",
                repository.as_str()
            );
        }
    };
    instruction.into()
}
