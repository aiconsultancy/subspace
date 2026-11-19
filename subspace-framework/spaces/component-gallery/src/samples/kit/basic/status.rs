//! GPUI Kit progress and disclosure basics: spinners, progress bars and
//! circles, collapsibles and accordions.

use gpui_kit::component::accordion::Accordion;
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::progress::{Progress, ProgressCircle};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::{App, Context, Entity, ParentElement as _, Styled as _, Window, div};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{self, Intent};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{Sample, Values, flag, sample, sized, store_flag};

const PROGRESS_VALUE: f32 = 60.;
const PROGRESS_WIDTH: f32 = 200.;
const DISCLOSURE_WIDTH: f32 = 260.;
const ACCORDION_WIDTH: f32 = 360.;
const COLLAPSIBLE: &str = "kit-collapsible";
/// The accordion's items: the stored key of each open flag, its title and body.
const SECTIONS: [(&str, &str, &str); 3] = [
    (
        "kit-accordion-0",
        "What is a fixture?",
        "Obviously fake data that a sample shows.",
    ),
    (
        "kit-accordion-1",
        "Does it read files?",
        "No. The gallery reads no configuration.",
    ),
    (
        "kit-accordion-2",
        "Can I copy a name?",
        "Yes, with the copy button on each card.",
    ),
];

pub(crate) fn spinners(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample("xsmall", Spinner::new().xsmall()),
        sample("small", Spinner::new().small()),
        sample("medium", Spinner::new()),
    ]
}

pub(crate) fn progress(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let bar = |id: &'static str, colour, label: &'static str| {
        sized(
            PROGRESS_WIDTH,
            Progress::new(id)
                .xsmall()
                .value(PROGRESS_VALUE)
                .color(colour)
                .accessibility_label(label),
        )
    };
    vec![
        sample(
            "Bar, accent",
            bar(
                "gallery-sample:progress-accent",
                tokens.accent,
                "Running sample",
            ),
        ),
        sample(
            "Bar, red",
            bar("gallery-sample:progress-red", tokens.red, "Failed sample"),
        ),
        sample(
            "Circle",
            ProgressCircle::new("gallery-kit:progress-circle")
                .value(PROGRESS_VALUE)
                .accessibility_label("Fixture upload"),
        ),
        sample(
            "Circle, loading",
            ProgressCircle::new("gallery-kit:progress-circle-loading")
                .loading(true)
                .accessibility_label("Fixture loading"),
        ),
    ]
}

pub(crate) fn collapsibles(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = gallery.retained.values.clone();
    let open = flag(&values, COLLAPSIBLE, false, cx);
    let icon = if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let trigger = action::action_button(
        "gallery-sample:collapsible",
        Intent::Quiet,
        Some(icon),
        "Details",
    )
    .on_click(move |_, _, cx| store_flag(&values, COLLAPSIBLE, !open, cx));
    let collapsible = Collapsible::new().open(open).child(trigger).content(
        v_flex()
            .pl_2()
            .text_color(tokens.muted)
            .child(div().child("Fixture details inside the disclosure.")),
    );
    vec![sample(
        "Interactive; click Details",
        sized(DISCLOSURE_WIDTH, collapsible),
    )]
}

/// Stores which accordion items are open after a click.
fn store_open(values: &Entity<Values>, open: &[usize], cx: &mut App) {
    for (at, (key, _, _)) in SECTIONS.iter().enumerate() {
        store_flag(values, key, open.contains(&at), cx);
    }
}

pub(crate) fn accordions(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = gallery.retained.values.clone();
    let mut accordion = Accordion::new("gallery-kit:accordion").bordered(true);
    for (key, title, body) in SECTIONS {
        let open = flag(&values, key, false, cx);
        accordion = accordion.item(move |item| item.open(open).title(title).child(body));
    }
    let accordion = accordion.on_toggle_click(move |open, _, cx| store_open(&values, open, cx));
    vec![sample(
        "Interactive, one open at a time",
        sized(ACCORDION_WIDTH, accordion),
    )]
}
