//! Explicit fixture seeds, absent from production builds. No environment hooks.
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, Context, Entity, Window};
use subspace_client_github::issue_report::{Draft, Error, IssuePaths, ReportPlan, TransportKind};

use crate::{Attachment, ReportIssue, report::Phase};

impl ReportIssue {
    /// Seed explicit synthetic path consent after opening the real report form.
    pub fn fixture_paths(&mut self, paths: IssuePaths, included: bool, cx: &mut Context<Self>) {
        self.set_path_diagnostics(paths, cx);
        self.set_include_paths(included, cx);
    }

    /// Measure the real Root before scrolling its fixed-height preview.
    /// Call outside a report update so Root can read the report during layout.
    pub fn fixture_preview_end(report: &Entity<Self>, window: &mut Window, cx: &mut App) {
        window.render_frame(cx);
        let body = report.read(cx).form.preview_body.clone();
        body.update(cx, |input, cx| {
            let end = input.text().len();
            input.set_selected_range(end..end, cx);
        });
    }

    /// Seed synthetic text for an isolated native evidence build.
    pub fn fixture_draft(&mut self, draft: &Draft, window: &mut Window, cx: &mut Context<Self>) {
        self.form
            .title
            .update(cx, |input, cx| input.set_value(draft.title(), window, cx));
        self.form.description.update(cx, |input, cx| {
            input.set_value(draft.description(), window, cx)
        });
        self.form
            .steps
            .update(cx, |input, cx| input.set_value(draft.steps(), window, cx));
        if draft.issue_type() != subspace_client_github::issue_report::IssueType::Unknown {
            self.form.kind.update(cx, |input, cx| {
                input.set_selected_value(&draft.issue_type().name().to_owned(), window, cx)
            });
        }
        self.state.fields = self.form.values(cx);
        self.state.phase = Phase::Editing;
        cx.notify();
    }

    /// Use a real prepared, synthetic client plan. The preview is never copied markup.
    pub fn fixture_preview(
        &mut self,
        plan: ReportPlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.form.set_preview(plan.title(), plan.body(), window, cx);
        self.state.phase = Phase::Review(Box::new(plan));
        cx.notify();
    }

    /// Seed a real browser result from its exact plan, without external I/O.
    /// Authenticated plans are not browser fixtures and are ignored.
    pub fn fixture_browser(
        &mut self,
        plan: ReportPlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if plan.transport() != TransportKind::Browser {
            return;
        }
        self.form.set_preview(plan.title(), plan.body(), window, cx);
        self.state.body_pending = plan.browser_fallback().clipboard_body().is_some();
        self.state.phase = Phase::Browser(plan.browser_fallback().clone());
        self.state.error = None;
        self.state.notice = Some("Simulated browser outcome. No issue was created, and the browser and clipboard were not used.".into());
        cx.notify();
    }

    /// Seed a typed simulated failure without a network request.
    pub fn fixture_error(&mut self, error: Error, cx: &mut Context<Self>) {
        self.state.phase = Phase::Editing;
        self.state.error = Some(format!("Simulated outcome: {error}"));
        cx.notify();
    }

    /// Seed only already-validated synthetic pixels.
    pub fn fixture_attachment(&mut self, attachment: Attachment, cx: &mut Context<Self>) {
        self.add_attachment(attachment, cx);
    }

    /// Set the real disclosure state without sending native input.
    pub fn fixture_diagnostics_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        self.state.diagnostics_expanded = expanded;
        cx.notify();
    }

    /// Seed a simulated created result. This never submits or opens the URL.
    pub fn fixture_created(&mut self, cx: &mut Context<Self>) {
        self.state.body_pending = false;
        self.state.phase = Phase::Created(format!("{}/1", self.repository.issues_url()));
        self.state.error = None;
        self.state.notice = Some(
            "Simulated created outcome. No issue was created. Browser and clipboard were not used."
                .into(),
        );
        cx.notify();
    }

    /// Show simulated copy progress. Never calls a clipboard operation.
    pub fn fixture_copy_progress(&mut self, copied: usize, cx: &mut Context<Self>) {
        let count = self.state.attachments.len();
        if count == 0 {
            return;
        }
        self.state.next_image = copied % count;
        self.state.notice = Some(format!(
            "Simulated copy progress: {copied} of {count} images. Clipboard was not used. Paste each image before copying the next."
        ));
        cx.notify();
    }

    /// Save actual validated pixels in the fixture-owned temporary folder.
    /// Folder selection is supplied by the fixture, not a native picker.
    pub fn fixture_save_images(&mut self, cx: &mut Context<Self>) {
        let result = self.state.attachments.save_in(&self.save_directory);
        match result {
            Ok(()) => {
                self.state.notice = Some(format!(
                    "Saved {} images to the fixture temporary folder. Folder selection was simulated; no clipboard or browser was used.",
                    self.state.attachments.len()
                ))
            }
            Err(error) => self.state.error = Some(error),
        }
        cx.notify();
    }
}
