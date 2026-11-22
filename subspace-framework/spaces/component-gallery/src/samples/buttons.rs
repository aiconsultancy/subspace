//! Buttons: `action` constructors, feedback actions and the panel service link.

mod recovery;
mod toolbar;

use gpui_kit::component::{Disableable as _, Selectable as _};
use gpui_kit::{Context, Window};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{self, ActionState, Intent};
use subspace_dashboard::icon_button::{IconButton, IconButtonSize};
use subspace_lucide::{IconName, SvgIcon};

pub(super) use self::recovery::{configure_action, recovery_action, retry_action, service_link};
pub(super) use self::toolbar::{latched_icon_buttons, menu_buttons, toolbar_buttons};
use super::{Sample, ignore_click, sample};
use crate::ComponentGallery;
use crate::catalog::Item;

/// Every intent with a fitting glyph, in the order the catalogue lists them.
const INTENTS: [(Intent, &str, SvgIcon); 6] = [
    (Intent::Accept, "Accept", IconName::Check),
    (Intent::Reject, "Reject", IconName::X),
    (Intent::Caution, "Caution", IconName::TriangleAlert),
    (Intent::Primary, "Primary", IconName::Play),
    (Intent::Neutral, "Neutral", IconName::Copy),
    (Intent::Quiet, "Quiet", IconName::Eye),
];
const ACTION_STATES: [(ActionState, &str); 4] = [
    (ActionState::Idle, "Idle"),
    (ActionState::Busy, "Busy"),
    (ActionState::Success, "Success"),
    (ActionState::Failure, "Failure"),
];
const SAVE_LABELS: [&str; 4] = ["Save", "Saving…", "Saved", "Save failed"];

const ACTION: &str = "board-kit/dashboard/src/action.rs";
const FEEDBACK: &str = "board-kit/dashboard/src/feedback.rs";

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "icon_button::IconButton",
        source: "board-kit/dashboard/src/icon_button.rs",
        used_in: "Shared icon-bearing actions across boards and the host. This live sample follows the global size. In Settings > Appearance, choose Small, Medium, Large or Extra large to view each size in turn.",
        samples: shared_icon_button,
        kit: None,
    },
    Item {
        name: "icon_button::latched",
        source: "board-kit/dashboard/src/icon_button.rs",
        used_in: "Independent toolbar on/off controls, including log Follow and Wrap. The paired states use the toolbar background and current global icon size. Choose each size in Settings > Appearance to compare them.",
        samples: latched_icon_buttons,
        kit: None,
    },
    Item {
        name: "action::action_button",
        source: ACTION,
        used_in: "Labelled actions in every board and the host.",
        samples: action_buttons,
        kit: None,
    },
    Item {
        name: "action::ActionState",
        source: ACTION,
        used_in: "Busy and result feedback on action buttons in IRC.",
        samples: action_states,
        kit: None,
    },
    Item {
        name: "action::menu_button",
        source: ACTION,
        used_in: "Menu-picker triggers in toolbars: Actions, Copilot Agents and GitHub.",
        samples: menu_buttons,
        kit: None,
    },
    Item {
        name: "action::header_button",
        source: ACTION,
        used_in: "Compact header-row actions with busy and disabled states; no board uses it at present.",
        samples: header_buttons,
        kit: None,
    },
    Item {
        name: "action::icon_button",
        source: ACTION,
        used_in: "Icon-only actions in Space Lab, Web, the detail panel, the composer, chat, shortcuts and toasts.",
        samples: icon_buttons,
        kit: None,
    },
    Item {
        name: "action::toolbar_button",
        source: ACTION,
        used_in: "Borderless toolbar actions; no board uses it at present.",
        samples: toolbar_buttons,
        kit: None,
    },
    Item {
        name: "feedback::retry_action",
        source: FEEDBACK,
        used_in: "Retry inside feedback::error_box, feedback::error_box_with_settings and feedback::error_banner.",
        samples: retry_action,
        kit: None,
    },
    Item {
        name: "feedback::recovery_action",
        source: FEEDBACK,
        used_in: "Recovery actions in the Terminal board and the host.",
        samples: recovery_action,
        kit: None,
    },
    Item {
        name: "feedback::configure_action",
        source: FEEDBACK,
        used_in: "The Configure action inside feedback::configuration and feedback::error_box_with_settings.",
        samples: configure_action,
        kit: None,
    },
    Item {
        name: "panel::service_link",
        source: "board-kit/dashboard/src/panel.rs",
        used_in: "Open-in-service links in the shared detail panel: Actions, Copilot Agents and GitHub.",
        samples: service_link,
        kit: None,
    },
];

pub(super) fn shared_icon_button(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        IconButtonSize::current(cx).label(),
        IconButton::new("gallery-sample:shared-icon-button")
            .icon(IconName::Search)
            .accessibility_label("Search")
            .on_click(ignore_click),
    )]
}

pub(super) fn action_buttons(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    INTENTS
        .iter()
        .enumerate()
        .map(|(index, (intent, label, icon))| {
            let button = action::action_button(
                ("gallery-sample:action-button", index),
                *intent,
                Some(*icon),
                *label,
            );
            sample(*label, button.on_click(ignore_click))
        })
        .chain(action_button_states())
        .collect()
}

/// The states callers chain onto `action_button`: no icon, disabled,
/// selected, and selected with toggled as GitHub's Quiet toggles use it.
fn action_button_states() -> [Sample; 4] {
    let viewed = |id: &'static str| {
        action::action_button(id, Intent::Quiet, Some(IconName::Eye), "Viewed").selected(true)
    };
    [
        sample(
            "No icon",
            action::action_button(
                "gallery-sample:action-button-plain",
                Intent::Neutral,
                None,
                "No icon",
            ),
        ),
        sample(
            "Disabled",
            action::action_button(
                "gallery-sample:action-button-disabled",
                Intent::Primary,
                Some(IconName::Save),
                "Save",
            )
            .disabled(true),
        ),
        sample("Selected", viewed("gallery-sample:action-button-selected")),
        sample(
            "Selected and toggled",
            viewed("gallery-sample:action-button-toggled").toggled(true),
        ),
    ]
}

pub(super) fn action_states(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    ACTION_STATES
        .iter()
        .enumerate()
        .map(|(index, (state, caption))| {
            let button = action::action_button(
                ("gallery-sample:action-state", index),
                Intent::Primary,
                Some(IconName::Save),
                state.label(SAVE_LABELS),
            );
            sample(*caption, state.apply(button.on_click(ignore_click)))
        })
        .collect()
}

pub(super) fn header_buttons(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    INTENTS
        .iter()
        .enumerate()
        .map(|(index, (intent, label, icon))| {
            let id = ("gallery-sample:header-button", index);
            sample(
                *label,
                action::header_button(id, *intent, Some(*icon), *label),
            )
        })
        .chain([sample(
            "Disabled",
            action::header_button(
                "gallery-sample:header-button-disabled",
                Intent::Neutral,
                Some(IconName::Square),
                "Stopping…",
            )
            .disabled(true),
        )])
        .collect()
}

pub(super) fn icon_buttons(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    INTENTS
        .iter()
        .enumerate()
        .map(|(index, (intent, label, icon))| {
            let id = ("gallery-sample:icon-button", index);
            sample(*label, action::icon_button(id, *intent, *icon, *label))
        })
        .chain([sample(
            "Disabled",
            action::icon_button(
                "gallery-sample:icon-button-disabled",
                Intent::Quiet,
                IconName::Trash,
                "Delete",
            )
            .disabled(true),
        )])
        .collect()
}
