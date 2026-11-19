use gpui_kit::component::{Disableable as _, Sizable as _, input::Textarea};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Render, Styled as _, Window, div, prelude::FluentBuilder as _,
};
use subspace_dashboard::{
    Tokens,
    action::{Intent, action_button},
    anchored,
    fields::{self, InputRole},
    icon_button::IconButton,
    rich,
};
use subspace_lucide::IconName;

use crate::{
    ReportIssue,
    report::Phase,
    submission::{review_instruction, submit_label},
};

impl Render for ReportIssue {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui_kit::IntoElement {
        let tokens = Tokens::current(cx);
        div()
            .id("report-issue")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_3()
            .min_w_0()
            .text_color(tokens.fg)
            .child(self.render_content(cx))
            .when_some(self.state.error.clone(), |view, error| {
                view.child(
                    div()
                        .text_color(tokens.red)
                        .child(rich::plain_text("report-error", &error)),
                )
            })
            .when_some(self.state.notice.clone(), |view, notice| {
                view.child(
                    div()
                        .text_color(tokens.muted)
                        .child(rich::plain_text("report-notice", &notice)),
                )
            })
            .when(self.state.image_busy, |view| {
                view.child("Waiting for image selection or save…")
            })
    }
}

// qual:allow(coupling, oi) reason: "False positive: private view methods share the retained ReportIssue owner in report.rs; there is no cross-feature access or independent state owner."
impl ReportIssue {
    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        match &self.state.phase {
            Phase::Editing | Phase::Preparing => self.render_form(cx).into_any_element(),
            Phase::Review(_) | Phase::Submitting => self.render_review(cx).into_any_element(),
            Phase::Created(url) => self.render_result(url, "Issue created. Images are not uploaded. Use Copy image for each image, then paste it into the issue in GitHub.", cx).into_any_element(),
            Phase::Browser(fallback) => self.render_result(fallback.url(), "Finish the report in GitHub. Opening an editor does not confirm issue creation.", cx).into_any_element(),
            Phase::Uncertain { opened } => render_uncertain(cx).child(retry_button(*opened, cx)).into_any_element(),
        }
    }

    fn render_form(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.state.busy();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(labelled(
                "Description (required)",
                Textarea::new(&self.form.description)
                    .accessibility_id("report-description")
                    .aria_label("Description (required)")
                    .small()
                    .disabled(busy),
            ))
            .child(labelled(
                "Title (optional)",
                fields::input(
                    &self.form.title,
                    InputRole::Configuration,
                    "Title (optional)",
                )
                .id("report-title")
                .disabled(busy),
            ))
            .child(labelled(
                "Type (optional)",
                anchored::select(&self.form.kind)
                    .id("report-type")
                    .accessibility_label("Type (optional)")
                    .placeholder("Unknown (no choice)")
                    .small()
                    .disabled(busy),
            ))
            .child(labelled(
                "Steps to reproduce (optional)",
                Textarea::new(&self.form.steps)
                    .accessibility_id("report-steps")
                    .aria_label("Steps to reproduce (optional)")
                    .small()
                    .disabled(busy),
            ))
            .child(self.render_attachment(cx))
            .child(self.render_diagnostics(cx))
            .child(
                action_button(
                    "report-review",
                    Intent::Primary,
                    Some(IconName::Eye),
                    "Review report",
                )
                .disabled(busy)
                .loading(matches!(self.state.phase, Phase::Preparing))
                .on_click(cx.listener(|this, _, window, cx| this.review(window, cx))),
            )
    }

    fn render_review(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.state.busy();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(format!(
                "Destination: {}. Images are not uploaded. Copy and paste each image into GitHub's browser editor.",
                self.repository.as_str()
            ))
            .child(review_instruction(&self.state.phase, &self.repository))
            .child(self.render_exact_text())
            .child(self.render_attachment(cx))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        action_button(
                            "report-edit",
                            Intent::Neutral,
                            Some(IconName::Pencil),
                            "Edit report",
                        )
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| this.edit(window, cx))),
                    )
                    .child(
                        action_button(
                            "report-submit",
                            Intent::Primary,
                            Some(IconName::Send),
                            submit_label(&self.state.phase),
                        )
                        .disabled(busy)
                        .loading(matches!(self.state.phase, Phase::Submitting))
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }

    fn render_exact_text(&self) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(labelled(
                "Exact title",
                Textarea::new(&self.form.preview_title)
                    .accessibility_id("report-preview-title")
                    .aria_label("Exact title")
                    .small()
                    .readonly(true),
            ))
            .child(labelled(
                "Exact issue body",
                Textarea::new(&self.form.preview_body)
                    .accessibility_id("report-preview-body")
                    .aria_label("Exact issue body")
                    .small()
                    .readonly(true),
            ))
    }

    fn render_result(&self, url: &str, message: &'static str, cx: &mut Context<Self>) -> Div {
        let url = url.to_owned();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(message)
            .child(rich::plain_text("report-result-url", &url))
            .child(
                IconButton::new("report-open-result")
                    .icon(IconName::ExternalLink)
                    .label("Open in GitHub")
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            )
            .when(matches!(self.state.phase, Phase::Browser(_)), |view| {
                view.child(self.render_browser_text(cx))
            })
            .child(self.render_attachment(cx))
    }

    fn render_browser_text(&self, cx: &mut Context<Self>) -> Div {
        div().flex().flex_col().gap_3()
            .child("For a blank GitHub editor, paste the title and body first. Choose I pasted the body to enable Copy image. Copy title, Copy body and Copy image each replace the clipboard. Paste each image before copying the next. You can restore either text at any time.")
            .child(self.render_exact_text())
            .when(self.state.body_pending, |view| view.child(action_button(
                "report-body-pasted", Intent::Neutral, Some(IconName::Check), "I pasted the body",
            ).disabled(self.state.busy()).on_click(cx.listener(|this, _, _, cx| this.acknowledge_body(cx)))))
            .child(div().flex().gap_2()
                .child(action_button("report-copy-title", Intent::Neutral, Some(IconName::Copy), "Copy title")
                    .disabled(self.state.busy()).on_click(cx.listener(|this, _, _, cx| this.copy_title(cx))))
                .child(action_button("report-copy-body", Intent::Neutral, Some(IconName::Copy), "Copy body")
                    .disabled(self.state.busy()).on_click(cx.listener(|this, _, _, cx| this.copy_body(cx)))))
    }
}

fn render_uncertain(cx: &mut Context<ReportIssue>) -> Div {
    div().flex().flex_col().gap_3()
        .child("Do not submit again until you have checked GitHub for the issue. A failed response can still mean the issue was created.")
        .child(action_button("report-check-github", Intent::Neutral, Some(IconName::ExternalLink), "Check GitHub issues")
            .on_click(cx.listener(|this, _, _, cx| this.check_github(cx))))
}

fn retry_button(opened: bool, cx: &mut Context<ReportIssue>) -> IconButton {
    action_button(
        "report-retry-checked",
        Intent::Caution,
        Some(IconName::RotateCcw),
        "I checked GitHub; review again",
    )
    .disabled(!opened)
    .on_click(cx.listener(|this, _, _, cx| this.retry_checked(cx)))
}

fn labelled(label: &'static str, control: impl gpui_kit::IntoElement) -> Div {
    div().flex().flex_col().gap_1().child(label).child(control)
}
