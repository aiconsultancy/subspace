use crate::Issues;
use gpui_kit::{AppContext as _, Context, Entity, FocusHandle, Subscription, Window};
use subspace_api_client::hub::{CommentDraft, CommentState};
use subspace_framework::dashboard::composer::Composer;

pub(super) struct Editor {
    pub(super) entity: Entity<Composer>,
    pub(super) parent: Option<String>,
    pub(super) notice_focus: FocusHandle,
    pub(super) state: CommentState,
    pub(super) confirmation: u64,
    pub(super) _subscriptions: Vec<Subscription>,
}

pub(super) fn make_composer(
    reply: bool,
    draft: Option<&CommentDraft>,
    window: &mut Window,
    cx: &mut Context<Issues>,
) -> Entity<Composer> {
    cx.new(|cx| {
        let mut composer = Composer::new(
            if reply { "Reply" } else { "New comment" },
            if reply {
                "Write a reply"
            } else {
                "Leave a comment"
            },
            window,
            cx,
        );
        composer.set_retained(true, cx);
        composer.set_images(true, cx);
        composer.set_image_byte_limit(subspace_api_client::media::MAX_IMAGE_BYTES, cx);
        composer.set_submit_label(if reply { "Reply" } else { "Comment" }, cx);
        composer.set_text(draft.map_or("", |d| d.body()), window, cx);
        composer
    })
}

impl Editor {
    pub(super) fn focus_after(
        &self,
        failed: bool,
        confirmed: bool,
        window: &mut Window,
        cx: &mut Context<Issues>,
    ) {
        if failed {
            self.notice_focus.focus(window, cx);
        } else if confirmed && self.parent.is_none() {
            self.entity.update(cx, |e, cx| e.focus(window, cx));
        }
    }
    fn retry_wakeup(&self, draft: &CommentDraft, cx: &mut Context<Issues>) {
        if self.state != *draft.state()
            && let Some(delay) = draft.retry_delay()
        {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
    }
    pub(super) fn sync(
        &mut self,
        draft: &CommentDraft,
        reason: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Issues>,
    ) -> bool {
        let confirmed = self.confirmation != draft.confirmation();
        self.retry_wakeup(draft, cx);
        self.state = draft.state().clone();
        self.confirmation = draft.confirmation();
        let label = if matches!(draft.state(), CommentState::Failed(_)) {
            "Retry"
        } else if self.parent.is_some() {
            "Reply"
        } else {
            "Comment"
        };
        self.entity.update(cx, |composer, cx| {
            if composer.text(cx) != draft.body() {
                composer.set_text(draft.body(), window, cx);
            }
            composer.set_busy(matches!(draft.state(), CommentState::Posting), cx);
            composer.set_readonly(draft.is_frozen() || reason.is_some(), cx);
            composer.set_submit_enabled(
                reason.is_none() && !draft.is_frozen() && draft.attachments_ready(),
                cx,
            );
            composer.set_optional(draft.has_attachments() && draft.attachments_ready(), cx);
            composer.set_submit_label(label, cx);
            composer.set_hint(
                if draft.has_pending_images() {
                    Some("Preparing attachments…".into())
                } else {
                    matches!(draft.state(), CommentState::Unclear { .. })
                        .then(|| "Check discussion before posting again.".into())
                },
                cx,
            );
        });
        confirmed
    }
}
