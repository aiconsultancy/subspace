//! Shared composition within the issue's structured panels. The controls and
//! surfaces retain Kit's geometry; only the labels and domain values live here.
//!
//! Every section of the reading column starts with the same heading: plain
//! semibold text at body size. Sections with rows put them in a filled Kit
//! GroupBox; an empty section keeps its heading and says so quietly, without
//! a box. Properties, Relations and Record are compact fact rows with no box.
//! A row's or a section's edit pencil stays quiet until the pointer is over
//! it or it has keyboard focus; it is always a real, tabbable control.
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants as _};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Div, InteractiveElement as _, IntoElement, ParentElement, Role, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, rems,
};
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::{Tokens, truncated};
use subspace_framework::lucide::SvgIcon;

/// The label column of a fact row, as the workspace's inspector facts have
/// it (wide enough for "Created by").
const FACT_LABEL_REM: f32 = 5.5;
/// A quiet control before the pointer or keyboard reaches it, and after.
/// At rest it is all but transparent rather than fully so: an element at
/// opacity 0 counts as hidden (the test harness will not click it), while
/// this one stays a visible, clickable target for pointer and harness alike.
const QUIET: f32 = 0.01;
const SHOWN: f32 = 1.;

/// The hover group of a fact row or a section, which shows its pencil.
pub(crate) fn hover_group(prefix: &str, label: &str) -> SharedString {
    format!("issue-{prefix}-hover:{label}").into()
}

/// An edit control that shows on its row's or section's hover and on
/// keyboard focus. It stays in the tab order with its accessible name.
pub(crate) fn quiet_until_hover(button: IconButton, group: SharedString) -> IconButton {
    button
        .opacity(QUIET)
        .group_hover(group, |style| style.opacity(SHOWN))
        .focus_visible(|style| style.opacity(SHOWN))
}

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

/// A section's heading: plain semibold text at body size, with no glyph.
/// Also used by the media sections, so every block on the page starts the
/// same way. `_icon` names the section's glyph for callers that still pass
/// one; the reading column shows none.
pub(crate) fn section_heading(
    title: impl Into<SharedString>,
    _icon: SvgIcon,
    tokens: &Tokens,
) -> Div {
    h_flex().gap_2().child(super::section_title(title, tokens))
}

/// The label cell of a fact row: muted, in the fixed label column.
pub(super) fn label_cell(tokens: &Tokens) -> Div {
    div()
        .w(rems(FACT_LABEL_REM))
        .flex_shrink_0()
        .text_color(tokens.muted)
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
        .text_color(tokens.fg_strong)
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

/// A label and its value, as compact as the workspace's inspector facts:
/// supporting text, a fixed label column, and one hover group so the row's
/// pencil shows under the pointer. The value column takes the rest of the
/// panel's width and may shrink below its text, so nothing runs past it.
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
        .group(hover_group(prefix, label))
        .w_full()
        .min_w_0()
        .items_start()
        .gap_3()
        .py_1()
        .text_size(tokens.secondary_size())
        .child(
            label_cell(tokens).child(subspace_framework::dashboard::rich::plain_text(
                format!("issue-{prefix}-label:{label}"),
                label,
            )),
        )
}

/// Fact rows one under another, with no box around them.
pub(super) fn grid() -> Div {
    v_flex().gap_0p5().w_full().min_w_0()
}
/// The issue page behind an open decision: it stays visible, but blocks the
/// pointer and is hidden from assistive technology, like an inert subtree
/// behind a modal dialog. Focus containment lives with the panel. The docks
/// and the pane's head wait behind it the same way.
pub(crate) fn behind_decision(page: Stateful<Div>, modal: bool) -> Stateful<Div> {
    if modal {
        page.relative()
            .role(Role::Group)
            .a11y_synthetic_children(|subtree| subtree.parent_node().set_hidden())
            .child(div().absolute().inset_0().occlude())
    } else {
        page
    }
}

/// The width an open decision or attachment panel takes beside the page.
pub(crate) fn side_width(issues: &crate::Issues, tokens: &Tokens) -> f32 {
    if issues.decision.is_some() {
        subspace_framework::dashboard::panel::clamp_width(
            f32::from(tokens.panel_width),
            issues.width,
        )
    } else {
        crate::media::text::panel_width(issues, tokens)
    }
}
