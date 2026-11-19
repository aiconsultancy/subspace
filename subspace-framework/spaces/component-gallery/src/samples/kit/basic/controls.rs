//! Interactive GPUI Kit basics: buttons, checkboxes, the clipboard button,
//! dropdown buttons and pagination. Controlled values live in the gallery's
//! `Values`.

use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _, DropdownButton};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::pagination::Pagination;
use gpui_kit::component::{Disableable as _, Selectable as _, Sizable as _, h_flex};
use gpui_kit::{Context, Entity, ParentElement as _, Styled as _, Window};
use subspace_dashboard::Tokens;
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{
    Sample, Values, flag, ignore_click, number, sample, store_flag, store_number,
};

const PAGES: usize = 12;
const CHECKED: &str = "kit-checkbox";
const PAGE: &str = "kit-page";

pub(super) fn values(gallery: &ComponentGallery) -> Entity<Values> {
    gallery.retained.values.clone()
}

pub(crate) fn buttons(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let button = |id: &'static str, label: &'static str| {
        Button::new(id).label(label).small().on_click(ignore_click)
    };
    vec![
        sample(
            "Variants",
            h_flex()
                .gap_2()
                .child(button("gallery-kit:button-primary", "Primary").primary())
                .child(button("gallery-kit:button-default", "Default"))
                .child(button("gallery-kit:button-outline", "Outline").outline())
                .child(button("gallery-kit:button-ghost", "Ghost").ghost())
                .child(button("gallery-kit:button-danger", "Delete").danger()),
        ),
        sample(
            "Icon, loading, disabled, selected",
            h_flex()
                .gap_2()
                .child(button("gallery-kit:button-icon", "Search").icon(IconName::Search))
                .child(button("gallery-kit:button-loading", "Saving…").loading(true))
                .child(button("gallery-kit:button-disabled", "Disabled").disabled(true))
                .child(button("gallery-kit:button-selected", "Selected").selected(true)),
        ),
        sample(
            "Button group",
            ButtonGroup::new("gallery-kit:button-group")
                .outline()
                .small()
                .child(Button::new("gallery-kit:group-day").label("Day"))
                .child(
                    Button::new("gallery-kit:group-week")
                        .label("Week")
                        .selected(true),
                )
                .child(Button::new("gallery-kit:group-month").label("Month")),
        ),
    ]
}

pub(crate) fn checkboxes(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = values(gallery);
    let checked = flag(&values, CHECKED, true, cx);
    vec![
        sample(
            "Interactive",
            Checkbox::new("gallery-kit:checkbox")
                .label("Show hidden fixtures")
                .checked(checked)
                .on_click(move |checked, _, cx| store_flag(&values, CHECKED, *checked, cx)),
        ),
        sample(
            "Disabled",
            Checkbox::new("gallery-kit:checkbox-disabled")
                .label("Locked option")
                .checked(true)
                .disabled(true),
        ),
    ]
}

pub(crate) fn clipboards(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Copies a fixture value",
        h_flex().gap_1().child("fixture-value").child(
            Clipboard::new("gallery-kit:clipboard")
                .value("fixture-value")
                .tooltip("Copy value"),
        ),
    )]
}

pub(crate) fn dropdown_buttons(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Main action and menu trigger",
        DropdownButton::new("gallery-kit:dropdown-button")
            .outline()
            .small()
            .menu_accessibility_label("More export options")
            .button(
                Button::new("gallery-kit:dropdown-button-main")
                    .label("Export")
                    .on_click(ignore_click),
            ),
    )]
}

pub(crate) fn paginations(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = values(gallery);
    let page = number(&values, PAGE, 1, cx);
    let compact = values.clone();
    vec![
        sample(
            "Interactive",
            Pagination::new("gallery-kit:pagination")
                .current_page(page)
                .total_pages(PAGES)
                .on_click(move |page, _, cx| store_number(&values, PAGE, *page, cx)),
        ),
        sample(
            "Compact",
            Pagination::new("gallery-kit:pagination-compact")
                .compact()
                .current_page(page)
                .total_pages(PAGES)
                .on_click(move |page, _, cx| store_number(&compact, PAGE, *page, cx)),
        ),
    ]
}
