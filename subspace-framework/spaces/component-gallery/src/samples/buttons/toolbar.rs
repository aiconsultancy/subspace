//! Menu-picker triggers and borderless toolbar actions, in the Buttons section.

use gpui_kit::component::Disableable as _;
use gpui_kit::{Context, ParentElement as _, Styled as _, Window, div};
use subspace_dashboard::Tokens;
use subspace_dashboard::action;
use subspace_dashboard::icon_button::{IconButton, IconButtonSize};
use subspace_lucide::IconName;

use super::INTENTS;
use crate::ComponentGallery;
use crate::samples::{Sample, ignore_click, sample};

const LABEL_SHOWN: [(bool, &str); 2] = [(false, "icon only"), (true, "with label")];
const MENU_ACTIVE: [(bool, &str); 2] = [(false, "Inactive"), (true, "Active (non-default)")];

pub(in crate::samples) fn latched_icon_buttons(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let size = IconButtonSize::current(cx).label();
    let mut samples: Vec<Sample> = [
        (false, "Off", "gallery-sample:latched-off"),
        (true, "On", "gallery-sample:latched-on"),
    ]
    .map(|(latched, caption, id)| {
        sample(
            format!("{caption}, {size}"),
            div().p_2().bg(tokens.bg).child(
                IconButton::new(id)
                    .icon(IconName::TextWrap)
                    .accessibility_label("Wrap")
                    .tooltip("Wrap log lines")
                    .latched(latched)
                    .on_click(ignore_click),
            ),
        )
    })
    .into();
    samples.push(sample(
        format!("Glyph latch on, as a workspace dock switch, {size}"),
        div().p_2().bg(tokens.bg).child(
            IconButton::new("gallery-sample:latched-glyph")
                .icon(IconName::PanelLeft)
                .accessibility_label("Hide navigator")
                .tooltip("Hide navigator")
                .latched_glyph(true)
                .on_click(ignore_click),
        ),
    ));
    samples
}

pub(in crate::samples) fn menu_buttons(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let disabled = action::menu_button(
        "gallery-sample:menu-button-disabled",
        IconName::ListFilter,
        "Severity",
        false,
    )
    .disabled(true);
    MENU_ACTIVE
        .iter()
        .enumerate()
        .map(|(index, (active, caption))| {
            let button = action::menu_button(
                ("gallery-sample:menu-button", index),
                IconName::ListFilter,
                "Severity",
                *active,
            );
            sample(*caption, button.on_click(ignore_click))
        })
        .chain([sample("Disabled", disabled)])
        .collect()
}

pub(in crate::samples) fn toolbar_buttons(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let cx = &*cx;
    let launch = |id: &'static str| {
        action::toolbar_button(
            id,
            IconName::Play,
            "Launch",
            (action::Intent::Primary, true),
            cx,
        )
    };
    INTENTS
        .iter()
        .enumerate()
        .flat_map(|(index, (intent, label, icon))| {
            LABEL_SHOWN.map(|(shown, caption)| {
                let id = (
                    "gallery-sample:toolbar-button",
                    index * 2 + usize::from(shown),
                );
                let button = action::toolbar_button(id, *icon, *label, (*intent, shown), cx);
                sample(format!("{label}, {caption}"), button)
            })
        })
        .chain([
            sample(
                "Disabled",
                launch("gallery-sample:toolbar-button-disabled").disabled(true),
            ),
            sample(
                "Loading",
                launch("gallery-sample:toolbar-button-loading")
                    .disabled(true)
                    .loading(true),
            ),
        ])
        .collect()
}
