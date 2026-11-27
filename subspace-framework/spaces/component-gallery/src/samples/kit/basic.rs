//! GPUI Kit basic components, in GPUI Kit's documentation order, with
//! Breadcrumb, Clipboard, Empty, Separator and Shimmer from its story gallery.

mod choices;
mod controls;
mod display;
mod messages;
mod status;
#[cfg(test)]
mod tests;
mod text;

use gpui_kit::component::message_scroller::MessageScrollerState;
use gpui_kit::component::questionnaire::QuestionnaireState;
use gpui_kit::component::slider::SliderState;
use gpui_kit::{App, Entity, Window};

use super::kit_source;
use crate::catalog::{Item, Kit};

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "gpui_kit::component::accordion::Accordion",
        source: kit_source!("accordion.rs"),
        used_in: "The theme designer's collapsible sections.",
        samples: status::accordions,
        kit: Some(Kit {
            paths: &["accordion::Accordion"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::alert::Alert",
        source: kit_source!("alert.rs"),
        used_in: "Not used by the Framework outside the gallery.",
        samples: display::alerts,
        kit: Some(Kit {
            paths: &["alert::Alert"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::attachment::Attachment",
        source: kit_source!("attachment.rs"),
        used_in: "Image and file drafts in the chat composer.",
        samples: messages::attachments,
        kit: Some(Kit {
            paths: &["attachment::Attachment"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::avatar::Avatar",
        source: kit_source!("avatar/avatar.rs"),
        used_in: "Behind the shared avatar helpers for people and chat members.",
        samples: messages::avatars,
        kit: Some(Kit {
            paths: &["avatar::Avatar", "avatar::AvatarGroup"],
            used: true,
            prefer: Some("avatar::person"),
        }),
    },
    Item {
        name: "gpui_kit::component::badge::Badge",
        source: kit_source!("badge.rs"),
        used_in: "Not used in Subspace yet; Subspace's count pill is the shared badge::badge.",
        samples: display::badges,
        kit: Some(Kit {
            paths: &["badge::Badge"],
            used: false,
            prefer: Some("badge::badge"),
        }),
    },
    Item {
        name: "gpui_kit::component::breadcrumb::Breadcrumb",
        source: kit_source!("breadcrumb.rs"),
        used_in: "Not used in Subspace yet.",
        samples: display::breadcrumbs,
        kit: Some(Kit {
            paths: &["breadcrumb::Breadcrumb"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::bubble::Bubble",
        source: kit_source!("bubble.rs"),
        used_in: "Chat bubbles through the shared chat_bubble helpers.",
        samples: messages::bubbles,
        kit: Some(Kit {
            paths: &["bubble::Bubble"],
            used: true,
            prefer: Some("chat_bubble::bubble"),
        }),
    },
    Item {
        name: "gpui_kit::component::button::Button",
        source: kit_source!("button/button.rs"),
        used_in: "Every Subspace button, through icon_button::IconButton and the action constructors.",
        samples: controls::buttons,
        kit: Some(Kit {
            paths: &["button::Button", "button::ButtonGroup"],
            used: true,
            prefer: Some("action::action_button"),
        }),
    },
    Item {
        name: "gpui_kit::component::checkbox::Checkbox",
        source: kit_source!("checkbox.rs"),
        used_in: "Not used in Subspace yet. Use it for independent choices in a list; forms use toggle::switch.",
        samples: controls::checkboxes,
        kit: Some(Kit {
            paths: &["checkbox::Checkbox"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::clipboard::Clipboard",
        source: kit_source!("clipboard.rs"),
        used_in: "The gallery's copy-name button; not used elsewhere in Subspace.",
        samples: controls::clipboards,
        kit: Some(Kit {
            paths: &["clipboard::Clipboard"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::collapsible::Collapsible",
        source: kit_source!("collapsible.rs"),
        used_in: "Disclosure sections in Help contents, chat failure details and Report issue diagnostics.",
        samples: status::collapsibles,
        kit: Some(Kit {
            paths: &["collapsible::Collapsible"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::button::DropdownButton",
        source: kit_source!("button/dropdown_button.rs"),
        used_in: "Not used in Subspace. Its menu attaches with .dropdown_menu, which Subspace routes through anchored, so this sample shows the split button without its menu.",
        samples: controls::dropdown_buttons,
        kit: Some(Kit {
            paths: &["button::DropdownButton"],
            used: false,
            prefer: Some("anchored::dropdown_menu"),
        }),
    },
    Item {
        name: "gpui_kit::component::empty::Empty",
        source: kit_source!("empty.rs"),
        used_in: "Not used in Subspace; empty states use the shared feedback::empty.",
        samples: display::empties,
        kit: Some(Kit {
            paths: &["empty::Empty"],
            used: false,
            prefer: Some("feedback::empty"),
        }),
    },
    Item {
        name: "gpui_kit::component::Icon",
        source: kit_source!("icon.rs"),
        used_in: "Every glyph, with Subspace's Lucide IconNames.",
        samples: display::icons,
        kit: Some(Kit {
            paths: &["Icon", "IconName"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::img",
        source: "GPUI: the img element, re-exported by gpui_kit",
        used_in: "Product logos and chat canvas drawings.",
        samples: display::images,
        kit: Some(Kit {
            paths: &["img("],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::kbd::Kbd",
        source: kit_source!("kbd.rs"),
        used_in: "Not used in Subspace yet; shortcuts show as text in menus and help.",
        samples: display::keys,
        kit: Some(Kit {
            paths: &["kbd::Kbd"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::label::Label",
        source: kit_source!("label.rs"),
        used_in: "Not used in Subspace yet; forms label fields with fields::form_field.",
        samples: text::labels,
        kit: Some(Kit {
            paths: &["label::Label"],
            used: false,
            prefer: Some("fields::form_field"),
        }),
    },
    Item {
        name: "gpui_kit::component::link::Link",
        source: kit_source!("link.rs"),
        used_in: "External links, through the shared links::external.",
        samples: text::links,
        kit: Some(Kit {
            paths: &["link::Link", "Link"],
            used: true,
            prefer: Some("links::external"),
        }),
    },
    Item {
        name: "gpui_kit::component::marker::Marker",
        source: kit_source!("marker.rs"),
        used_in: "Not used in Subspace yet; transcripts draw their own day separators.",
        samples: messages::markers,
        kit: Some(Kit {
            paths: &["marker::Marker"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::message::Message",
        source: kit_source!("message.rs"),
        used_in: "Chat and IRC transcript rows, through chat_bubble::message.",
        samples: messages::messages,
        kit: Some(Kit {
            paths: &["message::Message"],
            used: true,
            prefer: Some("chat_bubble::message"),
        }),
    },
    Item {
        name: "gpui_kit::component::message_scroller::MessageScroller",
        source: kit_source!("message_scroller.rs"),
        used_in: "Not used in the Framework; Spaces use it for chat transcripts.",
        samples: messages::message_scrollers,
        kit: Some(Kit {
            paths: &["message_scroller::MessageScroller"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::pagination::Pagination",
        source: kit_source!("pagination.rs"),
        used_in: "Not used in Subspace yet.",
        samples: controls::paginations,
        kit: Some(Kit {
            paths: &["pagination::Pagination"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::progress::Progress",
        source: kit_source!("progress/progress.rs"),
        used_in: "The update download bar in the About dialog.",
        samples: status::progress,
        kit: Some(Kit {
            paths: &["progress::Progress", "progress::ProgressCircle"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::questionnaire::Questionnaire",
        source: kit_source!("questionnaire/mod.rs"),
        used_in: "Not used in Subspace yet.",
        samples: messages::questionnaires,
        kit: Some(Kit {
            paths: &["questionnaire::Questionnaire"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::radio::Radio",
        source: kit_source!("radio.rs"),
        used_in: "Not used in Subspace yet. Use a radio group for a small visible set of exclusive choices.",
        samples: choices::radios,
        kit: Some(Kit {
            paths: &["radio::Radio", "radio::RadioGroup"],
            used: false,
            prefer: Some("segmented::control"),
        }),
    },
    Item {
        name: "gpui_kit::component::rating::Rating",
        source: kit_source!("rating.rs"),
        used_in: "Not used in Subspace yet.",
        samples: choices::ratings,
        kit: Some(Kit {
            paths: &["rating::Rating"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::separator::Separator",
        source: kit_source!("separator.rs"),
        used_in: "Separates format groups in the Markdown editor toolbar.",
        samples: text::separators,
        kit: Some(Kit {
            paths: &["separator::Separator"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::shimmer::ShimmerText",
        source: kit_source!("shimmer.rs"),
        used_in: "Not used in Subspace yet; the chat status line draws its own accent shimmer.",
        samples: text::shimmers,
        kit: Some(Kit {
            paths: &["shimmer::ShimmerText", "shimmer::ShimmerStyle"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::skeleton::Skeleton",
        source: kit_source!("skeleton.rs"),
        used_in: "Not used directly; loading placeholders use the shared feedback::skeleton.",
        samples: text::skeletons,
        kit: Some(Kit {
            paths: &["skeleton::Skeleton"],
            used: false,
            prefer: Some("feedback::skeleton"),
        }),
    },
    Item {
        name: "gpui_kit::component::slider::Slider",
        source: kit_source!("slider.rs"),
        used_in: "Density sliders in the theme designer and, through slider::control, Space Lab values.",
        samples: choices::sliders,
        kit: Some(Kit {
            paths: &["slider::Slider"],
            used: true,
            prefer: Some("slider::control"),
        }),
    },
    Item {
        name: "gpui_kit::component::spinner::Spinner",
        source: kit_source!("spinner.rs"),
        used_in: "Busy marks in the status bar and the About update panel.",
        samples: status::spinners,
        kit: Some(Kit {
            paths: &["spinner::Spinner"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::stepper::Stepper",
        source: kit_source!("stepper/stepper.rs"),
        used_in: "Not used in Subspace yet.",
        samples: choices::steppers,
        kit: Some(Kit {
            paths: &["stepper::Stepper"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::switch::Switch",
        source: kit_source!("switch.rs"),
        used_in: "Every switch, built with toggle::switch, which keeps its tracks contrast-safe.",
        samples: choices::switches,
        kit: Some(Kit {
            paths: &["switch::Switch"],
            used: true,
            prefer: Some("toggle::switch"),
        }),
    },
    Item {
        name: "gpui_kit::component::tag::Tag",
        source: kit_source!("tag.rs"),
        used_in: "Status pills through badge::Pill, and the gallery's used and unused marks.",
        samples: text::tags,
        kit: Some(Kit {
            paths: &["tag::Tag"],
            used: true,
            prefer: Some("badge::Pill"),
        }),
    },
    Item {
        name: "gpui_kit::component::text::TextView",
        source: kit_source!("text/compat.rs"),
        used_in: "Markdown and HTML bodies, through the shared rich text.",
        samples: text::text_views,
        kit: Some(Kit {
            paths: &["text::TextView", "TextView"],
            used: true,
            prefer: Some("rich::rich_text"),
        }),
    },
    Item {
        name: "gpui_kit::component::button::Toggle",
        source: kit_source!("button/toggle.rs"),
        used_in: "Toggles bold, italic and other formatting in the Markdown editor.",
        samples: choices::toggles,
        kit: Some(Kit {
            paths: &["button::Toggle", "button::ToggleGroup"],
            used: true,
            prefer: Some("icon_button::latched"),
        }),
    },
    Item {
        name: "gpui_kit::component::tooltip::Tooltip",
        source: kit_source!("tooltip.rs"),
        used_in: "Tooltips on buttons and truncated text, and IRC issue cards.",
        samples: text::tooltips,
        kit: Some(Kit {
            paths: &["tooltip::Tooltip", ".tooltip("],
            used: true,
            prefer: None,
        }),
    },
];

/// Retained entities of the basic samples.
pub(crate) struct State {
    pub(crate) slider: Entity<SliderState>,
    pub(crate) locked_slider: Entity<SliderState>,
    pub(crate) scroller: Entity<MessageScrollerState>,
    pub(crate) questionnaire: Entity<QuestionnaireState>,
}

impl State {
    pub(crate) fn new(_window: &mut Window, cx: &mut App) -> Self {
        Self {
            slider: choices::slider_state(cx),
            locked_slider: choices::slider_state(cx),
            scroller: messages::scroller_state(cx),
            questionnaire: messages::questionnaire_state(cx),
        }
    }
}
