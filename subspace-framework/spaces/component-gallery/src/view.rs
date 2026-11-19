//! The page: GPUI Kit's Sidebar for navigation and filtering, then the cards
//! of the chosen section, or of every section that matches the filter.
//!
//! Only the shown section's samples are built, so a long gallery stays fast.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    KeyContext, OperatingSystem, ParentElement as _, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div,
};
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::{DASHBOARD_CONTEXT, Dashboard as _, Tokens};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::browse::{self, Browse, MAX_MATCHES};
use crate::catalog::{Item, Section};

#[cfg(feature = "test-support")]
pub(crate) fn test_item(
    gallery: &ComponentGallery,
    name: &str,
    window: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Option<AnyElement> {
    let tokens = Tokens::current(cx);
    let (_, item) = crate::catalog::find(name)?;
    let samples = (item.samples)(gallery, &tokens, window, cx);
    let weak = cx.entity().downgrade();
    Some(card(item, samples, &tokens, weak, cx))
}

mod card;
mod navigation;

use card::card;
pub(crate) use card::reveal_button;
use navigation::navigation;

/// The whole board: navigation beside one scrolling page of cards.
pub(crate) fn page(
    gallery: &ComponentGallery,
    window: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Div {
    let tokens = Tokens::current(cx);
    let query = gallery.query(cx);
    let browse = Browse {
        query: &query,
        hide_unused: gallery.hide_unused,
    };
    let content = if query.is_empty() {
        section_page(gallery, browse, &tokens, window, cx)
    } else {
        matches_page(gallery, browse, &tokens, window, cx)
    };
    h_flex()
        .key_context(key_context(cx.platform_info().os))
        .track_focus(&gallery.focus)
        .relative()
        .items_stretch()
        .size_full()
        .min_h_0()
        .bg(tokens.bg)
        .text_color(tokens.fg)
        .text_size(tokens.row_font)
        .child(navigation(gallery, browse, cx))
        .child(
            v_flex()
                .id("component-gallery-page")
                .track_scroll(&gallery.scroll)
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_y_scroll()
                .p(tokens.pad_x)
                .gap_4()
                .children(content),
        )
        .children(gallery.menu_anchors.view())
}

/// The shared dashboard context plus the board's own, as other boards set it.
fn key_context(os: OperatingSystem) -> KeyContext {
    let mut context = KeyContext::new_with_defaults(os);
    context.add(DASHBOARD_CONTEXT);
    context.add(ComponentGallery::ID);
    context
}

/// The board toolbar: the latched "Hide unused GPUI Kit components" mode.
pub(crate) fn toolbar(
    gallery: &ComponentGallery,
    cx: &mut Context<ComponentGallery>,
) -> AnyElement {
    let (total, used) = browse::kit_counts();
    action::toolbar_button(
        "component-gallery-hide-unused",
        IconName::EyeOff,
        "Hide unused GPUI Kit components",
        (Intent::Neutral, false),
        cx,
    )
    .label("Hide unused")
    .tooltip(format!(
        "Hide the {} GPUI Kit components Subspace does not use",
        total - used
    ))
    .latched(gallery.hide_unused)
    .on_click(cx.listener(|this, _, _, cx| {
        let hide = !this.hide_unused;
        this.set_hide_unused(hide, cx);
    }))
    .into_any_element()
}

/// The chosen section: its heading and visible cards.
fn section_page(
    gallery: &ComponentGallery,
    browse: Browse<'_>,
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<AnyElement> {
    let section = browse::section(gallery.section);
    let items = browse.items(section);
    let summary = summary(section, items.len());
    let mut children = vec![heading(section, &summary, tokens).into_any_element()];
    children.extend(cards(gallery, &items, tokens, window, cx));
    if items.is_empty() {
        children.push(note(
            "No component in this section is used by Subspace.",
            tokens,
        ));
    }
    children
}

/// Every section with a card that matches the filter, at most [`MAX_MATCHES`] cards.
fn matches_page(
    gallery: &ComponentGallery,
    browse: Browse<'_>,
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<AnyElement> {
    let (groups, total) = browse.matches();
    let mut children = Vec::new();
    for (section, items) in groups {
        let summary = format!("{} · {}", section.library.title(), section.title);
        children.push(heading(section, &summary, tokens).into_any_element());
        children.extend(cards(gallery, &items, tokens, window, cx));
    }
    children.push(note(&match_note(total), tokens));
    children
}

fn match_note(total: usize) -> String {
    match total {
        0 => "No component matches the filter.".to_owned(),
        total if total > MAX_MATCHES => {
            format!("Showing {MAX_MATCHES} of {total} matches. Add words to narrow the filter.")
        }
        total => format!("{total} matching components."),
    }
}

fn summary(section: &Section, shown: usize) -> String {
    format!(
        "{} · {} · {shown} of {} components",
        section.library.title(),
        section.title,
        section.items.len()
    )
}

fn cards(
    gallery: &ComponentGallery,
    items: &[&'static Item],
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<AnyElement> {
    let weak = cx.entity().downgrade();
    items
        .iter()
        .map(|item| {
            let samples = (item.samples)(gallery, tokens, window, cx);
            card(item, samples, tokens, weak.clone(), cx)
        })
        .collect()
}

fn heading(section: &Section, summary: &str, tokens: &Tokens) -> impl IntoElement {
    div()
        .id(named("gallery-section", section.id))
        .test_support()
        .text_size(tokens.heading_font)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens.fg_strong)
        .child(summary.to_owned())
}

fn note(text: &str, tokens: &Tokens) -> AnyElement {
    div()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(text.to_owned())
        .into_any_element()
}

/// A stable element ID such as `gallery-item:action::icon_button`.
pub(crate) fn named(prefix: &str, name: &str) -> ElementId {
    ElementId::Name(format!("{prefix}:{name}").into())
}
