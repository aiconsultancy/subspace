use crate::{
    ComponentGallery,
    samples::{Sample, sample},
};
use gpui_kit::component::v_flex;
use gpui_kit::{Context, ParentElement as _, SharedString, Styled as _, Window};
use subspace_dashboard::{
    Tokens,
    action::{Intent, icon_button},
    animated_collapsible::{AnimatedCollapsible, CLOSED_PROGRESS, OPEN_PROGRESS},
    fields,
};
use subspace_lucide::IconName;
const MIDPOINT: f32 = 0.5;
const SUMMARY: &str = "Keep issue context beside the conversation so people can understand the work without leaving Channels.";

pub(in crate::samples) fn samples(
    board: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let mut samples: Vec<_> = [
        ("Collapsed", CLOSED_PROGRESS),
        ("Mid-way", MIDPOINT),
        ("Open", OPEN_PROGRESS),
    ]
    .into_iter()
    .map(|(label, progress)| {
        sample(
            label,
            AnimatedCollapsible::new(
                SharedString::from(format!("gallery-summary:{label}")),
                &board.retained.disclosure,
                fields::note(tokens, SUMMARY),
            )
            .preview(progress),
        )
    })
    .collect();
    let expanded = board.retained.disclosure.is_open();
    let toggle = icon_button(
        "gallery-summary-toggle",
        Intent::Quiet,
        IconName::ChevronDown,
        "Toggle summary",
    )
    .compact()
    .focus_on_click()
    .on_click(cx.listener(|board, _, _, cx| {
        let expanded = board.retained.disclosure.is_open();
        board.retained.disclosure.set_open(!expanded, cx);
        cx.notify();
    }));
    samples.push(sample(
        if expanded {
            "Interactive: open"
        } else {
            "Interactive: collapsed"
        },
        v_flex()
            .w_64()
            .child(toggle)
            .child(AnimatedCollapsible::new(
                "gallery-summary:interactive",
                &board.retained.disclosure,
                fields::note(tokens, SUMMARY),
            )),
    ));
    samples
}
