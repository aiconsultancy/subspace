//! The inspector's side panel: the picked element's component, identity,
//! source and box, with copy actions.

use gpui_kit::component::notification::{Notification, NotificationDelivery};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, Entity, FontWeight, Inspector, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div, rems,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::typography::{group_heading, heading_font};
use subspace_lucide::IconName;

use super::model::{ElementReport, edges_label, size_label};
use super::overlay::{PickShield, highlight};

const LABEL_WIDTH_REM: f32 = 5.5;
const TITLE: &str = "Inspector";
const PICKING_HINT: &str = "Click an element to pin it. Esc closes the inspector.";
const PINNED_HINT: &str = "Pinned. Choose Pick to inspect another element.";
const EMPTY_TITLE: &str = "Point at an element";
const EMPTY_HINT: &str =
    "Move the pointer over the window to see an element's bounds and component.";
const COPIED: &str = "Copied to the clipboard";

/// The panel for `report`, with the highlight over its element.
pub(super) fn render(
    report: Option<ElementReport>,
    (inspector, picking): (&Entity<Inspector>, bool),
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    if picking {
        window.request_animation_frame();
    }
    let tokens = Tokens::current(cx);
    let overlay = report
        .as_ref()
        .map(|report| highlight(report.layout.clone(), report.tag(), &tokens));
    v_flex()
        .id("inspector-panel")
        .test_support()
        .relative()
        .size_full()
        .bg(tokens.pane)
        .border_l_1()
        .border_color(tokens.line)
        .text_color(tokens.fg)
        .text_size(tokens.row_font)
        .child(header(inspector, picking, &tokens, cx))
        .child(body(report, picking, &tokens, cx))
        .children(overlay)
        .child(PickShield { picking })
        .into_any_element()
}

fn header(inspector: &Entity<Inspector>, picking: bool, tokens: &Tokens, cx: &App) -> Div {
    let pick = inspector.clone();
    h_flex()
        .h(tokens.toolbar_height)
        .flex_none()
        .px(tokens.pad_x)
        .gap_1()
        .border_b_1()
        .border_color(tokens.line)
        .child(
            div()
                .flex_1()
                .font_family(heading_font(cx))
                .font_weight(FontWeight::SEMIBOLD)
                .child(TITLE),
        )
        .child(
            action::icon_button(
                "inspector-pick",
                Intent::Quiet,
                IconName::SquareDashedMousePointer,
                "Pick",
            )
            .latched(picking)
            .on_click(move |_, window, cx| {
                pick.update(cx, |inspector, _| inspector.start_picking());
                window.refresh();
            }),
        )
        .child(
            action::icon_button("inspector-close", Intent::Quiet, IconName::X, "Close")
                .on_click(|_, window, cx| super::close(window, cx)),
        )
}

fn body(report: Option<ElementReport>, picking: bool, tokens: &Tokens, cx: &App) -> AnyElement {
    let content = match report {
        Some(report) => details(&report, picking, tokens, cx).into_any_element(),
        None => {
            subspace_dashboard::feedback::empty(tokens, "inspector-empty", EMPTY_TITLE, EMPTY_HINT)
                .into_any_element()
        }
    };
    v_flex()
        .id("inspector-body")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .p(tokens.pad_x)
        .gap_4()
        .child(content)
        .into_any_element()
}

fn details(report: &ElementReport, picking: bool, tokens: &Tokens, cx: &App) -> Div {
    let summary = report.summary();
    let hint = if picking { PICKING_HINT } else { PINNED_HINT };
    v_flex()
        .gap_4()
        .child(note(hint, tokens))
        .children(
            sections(report)
                .into_iter()
                .map(|(title, fields)| section(title, fields, cx)),
        )
        .child(
            h_flex().child(
                action::action_button(
                    "inspector-copy-all",
                    Intent::Neutral,
                    Some(IconName::Copy),
                    "Copy all",
                )
                .on_click(move |_, window, cx| copy(summary.clone(), window, cx)),
            ),
        )
}

/// One panel row: its id suffix, label, value and whether it can be copied.
struct Field {
    id: &'static str,
    label: &'static str,
    value: String,
    copyable: bool,
}

fn field(id: &'static str, label: &'static str, value: String, copyable: bool) -> Field {
    Field {
        id,
        label,
        value: if value.is_empty() {
            "—".into()
        } else {
            value
        },
        copyable,
    }
}

fn sections(report: &ElementReport) -> [(&'static str, Vec<Field>); 3] {
    let layout = &report.layout;
    let origin = format!(
        "{}, {}",
        f32::from(layout.bounds.origin.x).round(),
        f32::from(layout.bounds.origin.y).round()
    );
    [
        (
            "Component",
            vec![
                field("name", "Name", report.name().unwrap_or_default(), true),
                field("inside", "Inside", report.chain(), false),
            ],
        ),
        (
            "Element",
            vec![
                field(
                    "id",
                    "ID",
                    report.element_id.clone().unwrap_or_default(),
                    true,
                ),
                field("source", "Source", report.source.clone(), true),
                field("path", "ID path", report.id_path.clone(), true),
            ],
        ),
        (
            "Layout",
            vec![
                field("size", "Size", size_label(layout.bounds.size), false),
                field("origin", "Origin", origin, false),
                field("content", "Content", size_label(layout.content), false),
                field("padding", "Padding", edges_label(&layout.padding), false),
                field("border", "Border", edges_label(&layout.border), false),
                field("margin", "Margin", edges_label(&layout.margin), false),
            ],
        ),
    ]
}

fn note(text: &'static str, tokens: &Tokens) -> Div {
    div().text_color(tokens.muted).child(text)
}

fn section(title: &'static str, fields: Vec<Field>, cx: &App) -> Div {
    v_flex()
        .gap_1()
        .child(group_heading(div().pb_1(), cx).child(title))
        .children(fields.into_iter().map(|field| row(field, cx)))
}

/// `label` beside `value`; a copyable value has an icon-only Copy button
/// named after its label.
fn row(field: Field, cx: &App) -> AnyElement {
    let Field {
        id,
        label,
        value,
        copyable,
    } = field;
    let tokens = Tokens::current(cx);
    let text: SharedString = value.into();
    h_flex()
        .id(SharedString::from(format!("inspector-{id}")))
        .test_support()
        .w_full()
        .gap_2()
        .items_start()
        .child(
            div()
                .w(rems(LABEL_WIDTH_REM))
                .flex_none()
                .text_color(tokens.muted)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(tokens.fg_strong)
                .child(text.clone()),
        )
        .when(copyable, |row| {
            row.child(
                action::icon_button(
                    SharedString::from(format!("inspector-copy-{id}")),
                    Intent::Quiet,
                    IconName::Copy,
                    format!("Copy {}", label.to_lowercase()),
                )
                .on_click(move |_, window, cx| copy(text.to_string(), window, cx)),
            )
        })
        .into_any_element()
}

fn copy(text: String, window: &mut Window, cx: &mut App) {
    subspace_dashboard::links::copy(text, cx);
    window.push_notification(
        Notification::new()
            .id::<Inspector>()
            .message(COPIED)
            .delivery(NotificationDelivery::InApp),
        cx,
    );
}
