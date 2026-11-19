//! The board navigation: GPUI Kit's Sidebar, the filter and one group per
//! library, with the visible count of each section.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem};
use gpui_kit::{Context, IntoElement, ParentElement as _, SharedString, Styled as _, div};
use subspace_dashboard::fields::{self, InputRole};

use crate::ComponentGallery;
use crate::browse::{self, Browse};
use crate::catalog::{Library, SECTIONS, Section};

const SIDEBAR_WIDTH: f32 = 248.;

/// GPUI Kit's Sidebar: the filter, then one group per library.
pub(super) fn navigation(
    gallery: &ComponentGallery,
    browse: Browse<'_>,
    cx: &mut Context<ComponentGallery>,
) -> impl IntoElement {
    let filter = fields::input(
        &gallery.filter,
        InputRole::Search,
        "Filter components by name",
    )
    .cleanable(true);
    Sidebar::new("component-gallery-navigation")
        .collapsible(false)
        .w(gpui_kit::px(SIDEBAR_WIDTH))
        .header(div().w_full().child(filter))
        .children(
            Library::ALL
                .into_iter()
                .map(|library| library_group(gallery, library, browse, cx)),
        )
}

fn library_group(
    gallery: &ComponentGallery,
    library: Library,
    browse: Browse<'_>,
    cx: &mut Context<ComponentGallery>,
) -> SidebarGroup<SidebarMenu> {
    let label: SharedString = match library {
        Library::Kit => {
            let (total, used) = browse::kit_counts();
            format!("{} · {used} of {total} used", library.title()).into()
        }
        _ => library.title().into(),
    };
    let entries = SECTIONS
        .iter()
        .filter(|section| section.library == library)
        .map(|section| section_entry(gallery, section, browse, cx));
    SidebarGroup::new(label).child(SidebarMenu::new().children(entries))
}

fn section_entry(
    gallery: &ComponentGallery,
    section: &'static Section,
    browse: Browse<'_>,
    cx: &mut Context<ComponentGallery>,
) -> SidebarMenuItem {
    let count = browse.items(section).len();
    let active = browse.query.is_empty() && gallery.section == section.id;
    SidebarMenuItem::new(section.title)
        .accessibility_label(format!("{}, {count} components", section.title))
        .active(active)
        .suffix(move |_, cx| {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(count.to_string())
        })
        .on_click(cx.listener(move |this, _, window, cx| {
            this.select_section(section.id, window, cx);
        }))
}
