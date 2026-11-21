//! Menus: anchored dropdowns, native selects, the named menu anchors, the
//! popover anchor, the standard right-click menu and the repository picker.

pub(crate) mod owned;

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{Context, ParentElement as _, Styled as _, Window, px};
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::menu::{MenuContents, MenuOption};
use subspace_dashboard::{Glyph, Tokens, Tone, anchored};
use subspace_lucide::IconName;

use super::{Sample, ignore_click, sample, sized};
use crate::catalog::Item;
use crate::{ComponentGallery, MENU_ANCHOR};

const ANCHORED: &str = "board-kit/dashboard/src/anchored.rs";
const SELECT_WIDTH: f32 = 200.;

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "anchored::dropdown_menu",
        source: ANCHORED,
        used_in: "Native popup menus on a button in Board Lab, the help dock and the chat toolbar.",
        samples: dropdown_menu,
        kit: None,
    },
    Item {
        name: "anchored::select",
        source: ANCHORED,
        used_in: "Native selects in the Web manager, Board Lab and the host Settings.",
        samples: selects,
        kit: None,
    },
    Item {
        name: "anchored::color_select",
        source: ANCHORED,
        used_in: "Every colour in the theme designer.",
        samples: color_selects,
        kit: None,
    },
    Item {
        name: "menu::MenuAnchors",
        source: "board-kit/dashboard/src/menu/anchors.rs",
        used_in: "Filterable picker menus behind menu buttons in Actions, Copilot Agents, GitHub and chat.",
        samples: menu_anchors,
        kit: None,
    },
    Item {
        name: "anchored::Anchor",
        source: ANCHORED,
        used_in: "The retained host of Subspace's searchable menus. Escape and an outside press close it, and focus returns to the trigger.",
        samples: owned::anchors,
        kit: None,
    },
    Item {
        name: "anchored::form_popover",
        source: ANCHORED,
        used_in: "The Actions Filters panel stays open while you change values.",
        samples: form_popovers,
        kit: None,
    },
    Item {
        name: "row_menu::RowMenu",
        source: "board-kit/dashboard/src/row_menu.rs",
        used_in: "The standard right-click menu, in seven groups: shared list rows, group headers, the detail panel, navigator rows and the Space tab.",
        samples: owned::row_menus,
        kit: None,
    },
    Item {
        name: "repositories::RepositoryPicker",
        source: "board-kit/dashboard/src/repositories.rs",
        used_in: "The PRs board's Repositories control and Settings' Add repository…",
        samples: owned::repository_pickers,
        kit: None,
    },
];

/// The fixture options of the `menu::MenuAnchors` sample.
pub(crate) fn contents() -> MenuContents {
    MenuContents::new(
        "Gallery sample menu",
        "Filter the sample options…",
        vec![
            MenuOption::new("checked", "Checked option").checked(true),
            MenuOption::new("glyph", "Option with a glyph").icon(Glyph::new("●", Tone::Accent)),
            MenuOption::new("plain", "Plain option"),
        ],
    )
}

pub(super) fn popup_items(
    menu: PopupMenu,
    _: &mut Window,
    _: &mut Context<PopupMenu>,
) -> PopupMenu {
    menu.item(
        PopupMenuItem::new("Checked option")
            .checked(true)
            .on_click(ignore_click),
    )
    .item(PopupMenuItem::new("Plain option").on_click(ignore_click))
    .separator()
    .item(PopupMenuItem::new("Disabled option").disabled(true))
}

pub(super) fn dropdown_menu(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let trigger = action::action_button(
        "gallery-sample:dropdown-menu",
        Intent::Neutral,
        Some(IconName::ListFilter),
        "Options",
    );
    vec![sample(
        "Closed; click to open",
        anchored::dropdown_menu(trigger, popup_items),
    )]
}

pub(super) fn form_popovers(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let choice = gallery.retained.selects.popover.clone();
    let trigger = action::menu_button(
        "gallery-form-popover",
        IconName::Funnel,
        "Show filters",
        false,
    );
    let form = anchored::form_popover("gallery-filters")
        .trigger(trigger)
        .content(move |_, _, cx| {
            let popup = cx.weak_entity();
            let close = action::icon_button(
                "gallery-close-filters",
                Intent::Quiet,
                IconName::X,
                "Close filters",
            )
            .on_click(move |_, window, cx| {
                _ = popup.update(cx, |state, cx| state.dismiss(window, cx));
            });
            v_flex()
                .w(px(SELECT_WIDTH))
                .gap_2()
                .child(
                    h_flex()
                        .justify_between()
                        .child("Build profile")
                        .child(close),
                )
                .child(
                    anchored::select(&choice)
                        .small()
                        .accessibility_label("Build profile"),
                )
        });
    vec![sample("Choose a value, then close the form", form)]
}

pub(super) fn color_selects(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Swatch and hex value; click for the palette, HSLA and hex",
        sized(
            SELECT_WIDTH,
            anchored::color_select(&gallery.retained.selects.color)
                .accessibility_label("Fixture colour"),
        ),
    )]
}

pub(super) fn selects(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let selects = &gallery.retained.selects;
    vec![
        sample(
            "Value",
            sized(
                SELECT_WIDTH,
                anchored::select(&selects.value).accessibility_label("Build profile"),
            ),
        ),
        sample(
            "Placeholder",
            sized(
                SELECT_WIDTH,
                anchored::select(&selects.empty)
                    .placeholder("Choose a profile")
                    .accessibility_label("Unchosen build profile"),
            ),
        ),
        sample(
            "Small",
            sized(
                SELECT_WIDTH,
                anchored::select(&selects.small)
                    .small()
                    .accessibility_label("Small build profile"),
            ),
        ),
        sample(
            "Disabled",
            sized(
                SELECT_WIDTH,
                anchored::select(&selects.disabled)
                    .disabled(true)
                    .accessibility_label("Locked build profile"),
            ),
        ),
    ]
}

pub(super) fn menu_anchors(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let button = action::menu_button(
        "gallery-sample:menu-anchors-button",
        IconName::ListFilter,
        "Sample menu",
        false,
    )
    .on_click(cx.listener(|this, _, window, cx| this.open_menu(window, cx)));
    vec![sample(
        "Closed; click to open",
        gallery.menu_anchors.trigger(MENU_ANCHOR, button, cx),
    )]
}
