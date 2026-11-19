use gpui_kit::TestSupportExt as _;
use gpui_kit::component::Disableable as _;
use gpui_kit::{
    Context, Div, InteractiveElement as _, ObjectFit, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, div, img,
};
use subspace_dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_lucide::IconName;

use crate::{ReportIssue, attachments::MAX_ATTACHMENTS};

// qual:allow(coupling, oi) reason: "The attachment region shares the retained ReportIssue owner in report.rs."
impl ReportIssue {
    pub(super) fn render_attachment(&self, cx: &mut Context<Self>) -> Div {
        let count = self.state.attachments.len();
        let tokens = Tokens::current(cx);
        let count_label = format!("{count} of {MAX_ATTACHMENTS} images");
        let mut view = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_color(tokens.muted)
                    .child("Add screenshots of the issue. Annotate images if needed."),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("report-image-count")
                            .test_support()
                            .aria_label(count_label.clone())
                            .text_color(tokens.muted)
                            .child(count_label),
                    )
                    .children(self.images_mutable().then(|| {
                        action_button(
                            "report-choose-image",
                            Intent::Neutral,
                            Some(IconName::Image),
                            "Choose image…",
                        )
                        .disabled(self.state.busy() || self.state.attachments.is_full())
                        .on_click(cx.listener(|this, _, _, cx| this.choose_image(cx)))
                    })),
            );
        if count > 0 {
            view = view.child(self.render_thumbnails(cx))
                .child(div().text_color(tokens.orange).child("Check images before sending. Do not include sensitive hostnames, keys or customer data."))
                .child(self.render_image_actions(cx));
        }
        view
    }

    fn render_thumbnails(&self, cx: &mut Context<Self>) -> Div {
        let mutable = self.images_mutable();
        let busy = self.state.busy();
        div().flex().flex_wrap().gap_2().children(
            self.state
                .attachments
                .items()
                .iter()
                .enumerate()
                .map(|(ix, selected)| {
                    let id = selected.id;
                    div()
                        .id(("report-image", id))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .w_24()
                        .min_w_0()
                        .child(
                            img(selected.image.preview())
                                .w_full()
                                .h_16()
                                .object_fit(ObjectFit::Contain),
                        )
                        .children(mutable.then(|| {
                            action_button(
                                ("report-remove-image", id),
                                Intent::Neutral,
                                Some(IconName::X),
                                "Remove",
                            )
                            .accessibility_label(format!("Remove image {}", ix + 1))
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| this.remove_image(id, cx)))
                        }))
                }),
        )
    }

    fn render_image_actions(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(
                action_button(
                    "report-copy-image",
                    Intent::Neutral,
                    Some(IconName::Copy),
                    format!(
                        "Copy image {} of {}",
                        self.state.next_image + 1,
                        self.state.attachments.len()
                    ),
                )
                .disabled(self.state.busy() || self.state.body_pending)
                .on_click(cx.listener(|this, _, _, cx| this.copy_image(cx))),
            )
            .child(
                action_button(
                    "report-save-image",
                    Intent::Neutral,
                    Some(IconName::Download),
                    "Save images…",
                )
                .disabled(self.state.busy())
                .on_click(cx.listener(|this, _, _, cx| this.save_images(cx))),
            )
    }
}
