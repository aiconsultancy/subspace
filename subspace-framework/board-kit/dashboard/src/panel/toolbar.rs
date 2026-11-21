use super::{
    BAR_HEADING_SIZE_PX, BAR_PADDING_X_PX, PanelActions, PanelHeader, add_to_chat_button,
    icon_button, service_link,
};
use crate::{
    Tokens,
    icon_button::Metrics,
    list::ServiceLink,
    overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry},
    truncated,
    typography::heading_font,
    workspace::text_edge,
};
use gpui_kit::component::{h_flex, menu::PopupMenuItem};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, TestSupportExt as _, Window, div, px,
};
use std::rc::Rc;
use subspace_lucide::IconName;
#[cfg(test)]
mod tests;

// Positive logical-pixel estimate, replaced by OverflowToolbar's natural-width measurement.
const INITIAL_UNIT_WIDTH_PX: f32 = 1.;

type Callback = Rc<dyn Fn(&mut Window, &mut App)>;

pub(super) fn render(
    tokens: &Tokens,
    header: PanelHeader,
    actions: PanelActions,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let toolbar = window.use_keyed_state("detail-panel-actions", cx, |_, _| {
        OverflowToolbar::new(Vec::new())
    });
    let units = units(&header, &actions);
    toolbar.update(cx, |toolbar, cx| toolbar.set_units(units, cx));
    let close = actions.close.clone();
    // The bar is a workspace dock header: as tall as the pane's tab strip,
    // its title on the shared text edge, and Close last, outside overflow.
    h_flex()
        .id("panel-toolbar")
        .test_support()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .min_h(tokens.strip_height)
        .gap(Metrics::current(cx).gap())
        .pl(text_edge(tokens))
        .pr(px(BAR_PADDING_X_PX))
        .border_b_1()
        .border_color(tokens.line)
        .text_size(tokens.row_font)
        .child(title(tokens, header, cx))
        .child(
            div()
                .id("panel-actions")
                .test_support()
                .flex_shrink_1()
                .min_w(Metrics::current(cx).overflow_width())
                .max_w_full()
                .child(toolbar),
        )
        .child(
            icon_button("panel-close", IconName::X, "Close details")
                .on_click(move |_, window, cx| close(window, cx)),
        )
        .into_any_element()
}

fn title(tokens: &Tokens, header: PanelHeader, cx: &App) -> Div {
    h_flex()
        .flex_1()
        .min_w_16()
        .gap_2()
        .when_some(header.icon, |title, icon| {
            title.child(
                div()
                    .flex_shrink_0()
                    .text_color(tokens.glyph(&icon))
                    .child(icon.text),
            )
        })
        .child(
            div()
                .min_w_0()
                .flex_shrink_1()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(heading_font(cx))
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(tokens.heading_size(BAR_HEADING_SIZE_PX))
                .text_color(tokens.fg_strong)
                .child(header.kind),
        )
        .child(
            truncated::text("panel-header-id", header.label.unwrap_or(header.id))
                .flex_1()
                .text_color(tokens.muted),
        )
}

fn units(header: &PanelHeader, actions: &PanelActions) -> Vec<OverflowUnit> {
    let mut units = Vec::new();
    if let Some(step) = &actions.step {
        for (id, icon, label, direction) in [
            ("panel-previous", IconName::ArrowUp, "Previous item", -1),
            ("panel-next", IconName::ArrowDown, "Next item", 1),
        ] {
            let step = step.clone();
            units.push(callback_unit(
                id,
                icon,
                label,
                Rc::new(move |window, cx| step(direction, window, cx)),
            ));
        }
    }
    units.push(OverflowUnit::inline(
        "panel-add-to-chat",
        INITIAL_UNIT_WIDTH_PX,
        |_, _| add_to_chat_button(),
        vec![PopupEntry::new(|| {
            PopupMenuItem::new("Add to chat")
                .icon(IconName::MessageSquarePlus)
                .action(Box::new(crate::AddToChat))
        })],
    ));
    if let Some(link) = &header.link {
        units.push(link_unit(link.clone()));
    }
    units
}

fn callback_unit(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    callback: Callback,
) -> OverflowUnit {
    let popup = callback.clone();
    OverflowUnit::inline(
        id,
        INITIAL_UNIT_WIDTH_PX,
        move |_, _| {
            let callback = callback.clone();
            icon_button(id, icon, label).on_click(move |_, window, cx| callback(window, cx))
        },
        vec![PopupEntry::new(move || {
            let callback = popup.clone();
            PopupMenuItem::new(label)
                .icon(icon)
                .on_click(move |_, window, cx| callback(window, cx))
        })],
    )
}

fn link_unit(link: ServiceLink) -> OverflowUnit {
    let popup_link = link.clone();
    OverflowUnit::inline(
        "panel-link",
        INITIAL_UNIT_WIDTH_PX,
        move |_, _| service_link("panel-link", link.clone()),
        vec![PopupEntry::new(move || {
            let url = popup_link.url.clone();
            PopupMenuItem::new(popup_link.label.clone())
                .icon(IconName::ExternalLink)
                .on_click(move |_, _, cx| {
                    crate::links::open(&url, cx);
                })
        })],
    )
}
