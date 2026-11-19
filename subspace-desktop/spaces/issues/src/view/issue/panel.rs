//! Shared composition within the issue's structured panels. The controls and
//! surfaces retain Kit's geometry; only the labels and domain values live here.
//!
//! Every section of the page starts with the same heading: a muted glyph and
//! a semibold title. Sections with rows put them in a filled Kit GroupBox;
//! an empty section keeps its heading and says so quietly, without a box.
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants as _};
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Div, InteractiveElement as _, IntoElement, ParentElement, Role, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled, TestSupportExt as _, div,
};
use subspace_framework::dashboard::{Tokens, truncated};
use subspace_framework::lucide::SvgIcon;

pub(crate) struct Fact {
    pub(super) label: &'static str,
    pub(super) value: String,
}

impl Fact {
    pub(crate) fn new(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
        }
    }
}

/// A section's heading: a muted glyph and a semibold title. Also used by the
/// media sections, so every block on the page starts the same way.
pub(crate) fn section_heading(
    title: impl Into<SharedString>,
    icon: SvgIcon,
    tokens: &Tokens,
) -> Div {
    h_flex()
        .gap_2()
        .child(Icon::from(icon).small().text_color(tokens.muted))
        .child(super::section_title(title, tokens))
}

/// A section whose rows sit in a filled group.
pub(super) fn group(title: impl Into<SharedString>, icon: SvgIcon, tokens: &Tokens) -> GroupBox {
    section(title, icon, true, tokens)
}

/// A section that fills its group only when it has rows. With nothing in
/// it, the heading is followed by a quiet line on the heading's own edge
/// instead of an empty filled band.
pub(super) fn section(
    title: impl Into<SharedString>,
    icon: SvgIcon,
    filled: bool,
    tokens: &Tokens,
) -> GroupBox {
    GroupBox::new()
        .when(filled, |group| group.fill())
        .title(section_heading(title, icon, tokens))
}

/// Muted supporting text for an empty section or value.
pub(super) fn empty(text: impl Into<SharedString>, tokens: &Tokens) -> Div {
    div()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(text.into())
}

pub(crate) fn fact(fact: Fact, prefix: &str, tokens: &Tokens) -> AnyElement {
    let id = SharedString::from(format!("issue-{prefix}-value:{}", fact.label));
    fact_row(fact.label, fact.value.clone(), prefix, tokens)
        .child(value(id, fact.value, tokens))
        .into_any_element()
}

/// A value that keeps to its column: one line, cut with an ellipsis and the
/// whole value in a tooltip when the panel is too narrow.
pub(super) fn value(id: SharedString, text: impl Into<SharedString>, tokens: &Tokens) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .text_color(tokens.fg)
        .child({
            let text = text.into();
            truncated::styled(
                id.clone(),
                text.clone(),
                subspace_framework::dashboard::rich::plain_text(
                    format!("{id:?}:selectable"),
                    &text,
                ),
            )
        })
}

/// A label and its value. The value column takes the rest of the panel's
/// width and may shrink below its text, so nothing runs past the panel.
pub(super) fn fact_row(
    label: &'static str,
    value: String,
    prefix: &str,
    tokens: &Tokens,
) -> impl IntoElement + ParentElement {
    h_flex()
        .id(format!("issue-{prefix}:{label}"))
        .aria_label(format!("{label}: {value}"))
        .test_support()
        .w_full()
        .min_w_0()
        .items_start()
        .gap_3()
        .py_1()
        .child(div().w_24().flex_shrink_0().text_color(tokens.muted).child(
            subspace_framework::dashboard::rich::plain_text(
                format!("issue-{prefix}-label:{label}"),
                label,
            ),
        ))
}

pub(super) fn grid() -> Div {
    v_flex().gap_1().w_full().min_w_0()
}
/// The issue page behind an open decision: it stays visible, but blocks the
/// pointer and is hidden from assistive technology, like an inert subtree
/// behind a modal dialog. Focus containment lives with the panel.
pub(super) fn behind_decision(page: Stateful<Div>, modal: bool) -> Stateful<Div> {
    if modal {
        page.relative()
            .role(Role::Group)
            .a11y_synthetic_children(|subtree| subtree.parent_node().set_hidden())
            .child(div().absolute().inset_0().occlude())
    } else {
        page
    }
}

pub(super) fn side_width(issues: &crate::Issues, tokens: &Tokens) -> f32 {
    if issues.decision.is_some() {
        subspace_framework::dashboard::panel::clamp_width(
            f32::from(tokens.panel_width),
            issues.width,
        )
    } else {
        crate::media::text::panel_width(issues, tokens)
    }
}
