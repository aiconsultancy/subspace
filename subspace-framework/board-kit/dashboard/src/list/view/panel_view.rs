//! The detail panel of a list dashboard: the title, the properties, the
//! sections and the dashboard's extra content.

use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, FontWeight, IntoElement, ParentElement, SharedString,
    Styled, Window, div, px,
};

use super::super::{
    Body, Detail, ListDashboard, ListState, PANEL_TITLE_MARGIN_PX, PANEL_TITLE_SIZE_PX,
    PROP_LABEL_WIDTH_PX, PROP_ROW_MIN_HEIGHT_PX, Row, SECTION_TOP_MARGIN_PX,
    SECTION_TOP_PADDING_PX, Section, update,
};
use crate::panel::{self, PanelActions, PanelBody, PanelHeader};
use crate::rich::{self, Format};
use crate::tokens::Tokens;
use crate::typography::heading_font;

const SECTION_HEADING_SIZE_PX: f32 = 13.;

/// A panel section's text.
// qual:api - dashboards draw their own panel sections with it.
pub fn render_body(id: impl Into<ElementId>, body: &Body, tokens: &Tokens) -> AnyElement {
    match body {
        Body::Text(text) => div()
            .text_color(tokens.fg)
            .whitespace_normal()
            .child(rich::plain_text(id, text))
            .into_any_element(),
        Body::Markdown(text) => rich::rich_text(id, Format::Markdown, text).into_any_element(),
        Body::Html(text) => rich::rich_text(id, Format::Html, text).into_any_element(),
    }
}

/// The label and value pairs at the top of the panel.
fn panel_props(detail: &Detail, tokens: &Tokens) -> Div {
    let id = detail.id.clone();
    v_flex()
        .text_size(tokens.row_font)
        .children(detail.props.iter().enumerate().map(|(at, (label, value))| {
            h_flex()
                .min_h(px(PROP_ROW_MIN_HEIGHT_PX))
                .gap_3()
                .child(
                    div()
                        .w(px(PROP_LABEL_WIDTH_PX))
                        .flex_shrink_0()
                        .text_color(tokens.muted)
                        .child(label.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(tokens.fg_strong)
                        // Selectable, so the panel's menu can copy a value
                        // or send it to the chat.
                        .child(rich::plain_text(
                            ElementId::Name(format!("prop-{id}-{at}").into()),
                            value,
                        )),
                )
        }))
}

/// One headed block of text under the properties.
fn panel_section(
    detail_id: &SharedString,
    index: usize,
    section: &Section,
    tokens: &Tokens,
    heading_font: &SharedString,
) -> Div {
    v_flex()
        .mt(px(SECTION_TOP_MARGIN_PX))
        .pt(px(SECTION_TOP_PADDING_PX))
        .gap_1()
        .border_t_1()
        .border_color(tokens.line)
        .text_size(tokens.row_font)
        .child(
            div()
                .font_family(heading_font.clone())
                .text_size(tokens.heading_size(SECTION_HEADING_SIZE_PX))
                .text_color(tokens.muted)
                .child(section.heading.clone()),
        )
        .child(render_body(
            ElementId::Name(format!("section-{detail_id}-{index}").into()),
            &section.body,
            tokens,
        ))
}

/// Everything the panel scrolls: the title, the properties, the sections
/// and the dashboard's extra content.
fn panel_body(
    detail: &Detail,
    extra: Option<AnyElement>,
    tokens: &Tokens,
    heading_font: SharedString,
    content_owns_scroll: bool,
) -> Div {
    let sections =
        detail.sections.iter().enumerate().map(|(index, section)| {
            panel_section(&detail.id, index, section, tokens, &heading_font)
        });
    v_flex()
        .when(content_owns_scroll, |body| {
            body.flex_1().min_h_0().overflow_hidden()
        })
        .child(
            div()
                .flex_shrink_0()
                .mb(px(PANEL_TITLE_MARGIN_PX))
                .font_family(heading_font.clone())
                .text_size(tokens.heading_size(PANEL_TITLE_SIZE_PX))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(detail.title.clone()),
        )
        .child(panel_props(detail, tokens).flex_shrink_0())
        .children(sections)
        .children(extra.map(|extra| {
            v_flex()
                .mt(px(SECTION_TOP_MARGIN_PX))
                .when(content_owns_scroll, |extra| {
                    extra.flex_1().min_h_0().overflow_hidden()
                })
                .child(extra)
        }))
}

/// Closes the panel, from the panel's own back and close buttons.
fn close_callback<D: ListDashboard>(
    cx: &mut Context<D>,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let entity = cx.entity().downgrade();
    move |_: &mut Window, cx: &mut App| {
        _ = entity.update(cx, |this: &mut D, cx| {
            update(this, cx, ListState::close_detail);
        });
    }
}

pub(crate) fn detail_panel<D: ListDashboard>(
    board: &D,
    row: &Row,
    window: &mut Window,
    cx: &mut Context<D>,
) -> Option<AnyElement> {
    let detail = board.detail(&row.key)?;
    let extra = board.panel_extra(&row.key, cx);
    let tokens = Tokens::current(cx);
    let content_owns_scroll = board.panel_content_owns_scroll(&row.key);
    let body = PanelBody {
        content: panel_body(
            &detail,
            extra,
            &tokens,
            heading_font(cx),
            content_owns_scroll,
        )
        .into_any_element(),
        footer: board.panel_footer(&row.key, cx),
        scroll: board.panel_scroll_handle(&row.key),
        content_owns_scroll,
    };
    let close = close_callback(cx);
    let entity = cx.entity().downgrade();
    let step: panel::StepCallback = Rc::new(move |step, _, cx| {
        _ = entity.update(cx, |this: &mut D, cx| {
            update(this, cx, |list| {
                list.step_row(step);
            });
        });
    });
    let header = PanelHeader {
        icon: Some(row.icon.clone()),
        kind: detail.kind,
        id: detail.id,
        label: board.panel_header_label(&row.key),
        link: detail.link,
    };
    Some(panel::frame(
        header,
        body,
        PanelActions::new(close, Some(step)),
        window,
        cx,
    ))
}
