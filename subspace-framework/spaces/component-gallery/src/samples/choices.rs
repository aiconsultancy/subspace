//! Choices: segmented controls, tabs, switches in rows and the numeric slider.

pub(crate) mod dock;
mod resource_tabs;
mod switches;

use gpui_kit::component::Disableable as _;
use gpui_kit::{Context, Window};
use subspace_dashboard::segmented::{self, Segment};
use subspace_dashboard::tabs::{self, TabSpec, TabStatus};
use subspace_dashboard::{Tokens, slider, toggle};
use subspace_lucide::IconName;

pub(super) use self::switches::switches;
use super::{Sample, ignore_index, sample, sized};
use crate::ComponentGallery;
use crate::catalog::Item;

const TAB_COUNT: u64 = 12;
const TOGGLE_WIDTH: f32 = 260.;
const SLIDER_WIDTH: f32 = 260.;
const LONG_LABEL: &str = "An independent option with a long label that wraps in a narrow panel instead of pushing the switch out";

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "segmented::control",
        source: "board-kit/dashboard/src/segmented.rs",
        used_in: "Exclusive choices in Actions dialogs, Board Lab and GitHub diff.",
        samples: segmented_controls,
        kit: None,
    },
    Item {
        name: "tabs::tab_bar",
        source: "board-kit/dashboard/src/tabs.rs",
        used_in: "Section and resource tabs with tabs::tab in Actions, Board Lab, Channels, GitHub, Issues, Terminal, chat, logs and search.",
        samples: tab_bars,
        kit: None,
    },
    Item {
        name: "tabs::unread_dot",
        source: "board-kit/dashboard/src/tabs/paint.rs",
        used_in: "A passive unread mark in shared tabs and Channels conversation titles.",
        samples: unread_marks,
        kit: None,
    },
    Item {
        name: "tabs::dock::area",
        source: "board-kit/dashboard/src/tabs/dock.rs",
        used_in: "Channels conversation docks: the shared tab appearance with native splits, dragging and close callbacks.",
        samples: dock::samples,
        kit: None,
    },
    Item {
        name: "toggle::switch",
        source: "board-kit/dashboard/src/toggle.rs",
        used_in: "Every switch: Settings, the theme designer, search sources, Report issue, the native grid and board forms in Web. Its off track, on track and knob keep 3:1 on each surface shown.",
        samples: switches,
        kit: None,
    },
    Item {
        name: "toggle::toggle_row",
        source: "board-kit/dashboard/src/toggle.rs",
        used_in: "Independent boolean options in forms: Web, Settings, search sources and Report issue.",
        samples: toggle_rows,
        kit: None,
    },
    Item {
        name: "slider::control",
        source: "board-kit/dashboard/src/slider.rs",
        used_in: "Numeric project values in the Board Lab project view.",
        samples: sliders,
        kit: None,
    },
];

pub(super) fn segments() -> [Segment; 3] {
    [
        Segment::new("Day"),
        Segment::new("Week"),
        Segment::new("Month"),
    ]
}

pub(super) fn icon_segments() -> [Segment; 3] {
    [
        Segment::new("Table").icon(IconName::Table),
        Segment::new("Chart").icon(IconName::ChartArea),
        Segment::new("Layers").icon(IconName::Layers),
    ]
}

pub(super) fn segmented_controls(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "No choice yet",
            segmented::control(
                "gallery-sample:segmented-none",
                segments(),
                None,
                tokens,
                ignore_index,
            ),
        ),
        sample(
            "Selected",
            segmented::control(
                "gallery-sample:segmented-selected",
                segments(),
                Some(1),
                tokens,
                ignore_index,
            ),
        ),
        sample(
            "With icons",
            segmented::control(
                "gallery-sample:segmented-icons",
                icon_segments(),
                Some(0),
                tokens,
                ignore_index,
            ),
        ),
    ]
}

/// A tab bar whose first tab is the sample and is selected.
pub(super) fn bar(id: &'static str, first: TabSpec) -> tabs::TabBar {
    tabs::tab_bar(id)
        .selected_index(0)
        .child(tabs::tab(first))
        .child(tabs::tab(TabSpec::new("Settings", IconName::Settings)))
}

pub(super) fn tab_bars(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let issues = || TabSpec::new("Issues", IconName::ListChecks);
    vec![
        resource_tabs::sample(gallery, cx),
        sample("Icon, selected", bar("gallery-sample:tabs-icon", issues())),
        sample(
            "Plain",
            bar("gallery-sample:tabs-plain", TabSpec::plain("Overview")),
        ),
        sample(
            "Count",
            bar(
                "gallery-sample:tabs-count",
                issues().status(TabStatus::count(TAB_COUNT, tokens.muted)),
            ),
        ),
        sample(
            "Busy",
            bar(
                "gallery-sample:tabs-busy",
                issues().status(TabStatus::busy(tokens.muted)),
            ),
        ),
        sample(
            "Count and busy",
            bar(
                "gallery-sample:tabs-count-busy",
                issues().status(TabStatus::count_and_busy(TAB_COUNT, tokens.muted)),
            ),
        ),
        sample(
            "Disabled",
            bar("gallery-sample:tabs-disabled", issues().disabled(true)),
        ),
    ]
}

pub(super) fn toggle_rows(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let switch = |id: &'static str, checked: bool| {
        toggle::switch(id, cx)
            .checked(checked)
            .on_click(|_, _, _| {})
    };
    vec![
        sample(
            "On",
            sized(
                TOGGLE_WIDTH,
                toggle::toggle_row("Notifications", switch("gallery-sample:toggle-on", true)),
            ),
        ),
        sample(
            "Off",
            sized(
                TOGGLE_WIDTH,
                toggle::toggle_row(
                    "Background updates",
                    switch("gallery-sample:toggle-off", false),
                ),
            ),
        ),
        sample(
            "Disabled",
            sized(
                TOGGLE_WIDTH,
                toggle::toggle_row(
                    "Managed by policy",
                    switch("gallery-sample:toggle-disabled", true).disabled(true),
                ),
            ),
        ),
        sample(
            "Long label wraps",
            sized(
                TOGGLE_WIDTH,
                toggle::toggle_row(LONG_LABEL, switch("gallery-sample:toggle-long", false)),
            ),
        ),
    ]
}

pub(super) fn sliders(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let sliders = &gallery.retained.sliders;
    let value = sliders.value.iter().map(|state| {
        sample(
            "Value",
            sized(
                SLIDER_WIDTH,
                slider::control("gallery-sample:slider-value", "Opacity", state, tokens),
            ),
        )
    });
    let disabled = sliders.disabled.iter().map(|state| {
        sample(
            "Disabled",
            sized(
                SLIDER_WIDTH,
                slider::control(
                    "gallery-sample:slider-disabled",
                    "Locked opacity",
                    state,
                    tokens,
                ),
            ),
        )
    });
    value.chain(disabled).collect()
}

pub(super) fn unread_marks(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample("Unread items", tabs::unread_dot(tokens))]
}
