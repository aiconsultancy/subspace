//! Flow's job inspector: the selected job's result and steps, in place of
//! the run's inspector.

use gpui_kit::component::h_flex;
use gpui_kit::{
    ClickEvent, Context, InteractiveElement as _, ParentElement as _, Styled as _,
    TestSupportExt as _, div,
};
use subspace_framework::dashboard::rich::plain_text;
use subspace_framework::dashboard::{Tokens, workspace};

use super::{footer, job_note, shows_run};
use crate::graph::GraphJob;
use crate::{Actions, lanes};

/// Flow's selected job: its result and its steps. Its footer carries the
/// writes on the run when that run is the list's selection.
pub(super) fn job_inspector(
    this: &Actions,
    job: &GraphJob,
    cx: &mut Context<Actions>,
) -> workspace::Inspector {
    let tokens = Tokens::current(cx);
    let close = cx.listener(|this, _: &ClickEvent, _, cx| {
        this.detail.selected_job = None;
        cx.notify();
    });
    let note = h_flex()
        .id("actions-job-status")
        .test_support()
        .gap_1p5()
        .text_size(tokens.secondary_size())
        .child(lanes::status_icon(&job.status, &tokens))
        .child(job_note(job));
    let steps = workspace::section("Steps", cx).child(
        div()
            .min_w_0()
            .child(plain_text("actions-flow-job-detail", &job.steps)),
    );
    let inspector = workspace::Inspector::new("actions-job-inspector", job.name.clone())
        .on_close("Close job", close)
        .child(note)
        .child(steps);
    let selected = this.selected();
    if shows_run(selected, this.detail.target.as_ref()) {
        inspector.footer(footer(this, selected))
    } else {
        inspector
    }
}
