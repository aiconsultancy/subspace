//! GPUI Kit menus and tabs: popup and context menus through Subspace's
//! `anchored` boundary, the native OS menu and every tab bar variant.

use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::{
    Context, InteractiveElement as _, ParentElement as _, Styled as _, Window, actions, div,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::anchored::{self, AnchoredContextMenuExt as _};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{Sample, ignore_click, number, sample, store_number};

actions!(component_gallery, [NativeSampleCommand]);

const TABS: [&str; 3] = ["Overview", "Activity", "Settings"];
const TAB_KEY: &str = "kit-tabs";
const AREA: (f32, f32) = (220., 64.);

fn items(menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>) -> PopupMenu {
    menu.item(
        PopupMenuItem::new("Rename…")
            .icon(IconName::Pencil)
            .on_click(ignore_click),
    )
    .item(
        PopupMenuItem::new("Duplicate")
            .icon(IconName::Copy)
            .on_click(ignore_click),
    )
    .item(
        PopupMenuItem::new("Pinned")
            .checked(true)
            .on_click(ignore_click),
    )
    .separator()
    .item(
        PopupMenuItem::new("Delete")
            .icon(IconName::Trash)
            .disabled(true),
    )
}

pub(crate) fn menus(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let trigger = action::action_button(
        "gallery-kit:menu",
        Intent::Neutral,
        Some(IconName::Ellipsis),
        "Actions",
    )
    .dropdown_caret(true);
    let area = div()
        .id("gallery-kit:context-menu")
        .w(gpui_kit::px(AREA.0))
        .h(gpui_kit::px(AREA.1))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(tokens.line)
        .rounded(tokens.control_radius)
        .text_color(tokens.muted)
        .child("Right-click here")
        .anchored_context_menu(items);
    vec![
        sample(
            "Dropdown, through anchored::dropdown_menu",
            anchored::dropdown_menu(trigger, items),
        ),
        sample("Context menu, through anchored", area),
    ]
}

pub(crate) fn native_menus(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let button = Button::new("gallery-kit:native-menu")
        .label("Show native menu")
        .small()
        .outline()
        .on_click(|event, window, cx| {
            NativeMenu::new()
                .menu("Fixture command", Box::new(NativeSampleCommand))
                .separator()
                .menu_with_check("Checked command", true, Box::new(NativeSampleCommand))
                .show(event.position(), window, cx);
        });
    vec![sample("The operating system's own menu", button)]
}

pub(crate) fn tabs(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = &gallery.retained.values;
    let selected = number(values, TAB_KEY, 0, cx);
    let bar = |id: &'static str, style: fn(TabBar) -> TabBar| {
        let values = values.clone();
        style(TabBar::new(id))
            .small()
            .selected_index(selected)
            .children(TABS.map(|label| Tab::new().label(label)))
            .on_click(move |index: &usize, _, cx| store_number(&values, TAB_KEY, *index, cx))
    };
    vec![
        sample("Default", bar("gallery-kit:tabs-default", |bar| bar)),
        sample(
            "Underline",
            bar("gallery-kit:tabs-underline", TabBar::underline),
        ),
        sample("Pill", bar("gallery-kit:tabs-pill", TabBar::pill)),
        sample("Outline", bar("gallery-kit:tabs-outline", TabBar::outline)),
        sample(
            "Segmented",
            bar("gallery-kit:tabs-segmented", TabBar::segmented),
        ),
        sample(
            "Disabled tab",
            v_flex().child(
                TabBar::new("gallery-kit:tabs-disabled")
                    .small()
                    .selected_index(0)
                    .child(Tab::new().label("Enabled"))
                    .child(Tab::new().label("Disabled").disabled(true)),
            ),
        ),
    ]
}
