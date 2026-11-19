//! Interactive GPUI Kit choices: radios, ratings, sliders, steppers,
//! switches and toggles. Controlled values live in the gallery's `Values`.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::{Toggle, ToggleGroup};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::rating::Rating;
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::stepper::{Stepper, StepperItem};
use gpui_kit::{App, AppContext as _, Context, Entity, ParentElement as _, Window};
use subspace_dashboard::{Tokens, toggle};
use subspace_lucide::IconName;

use super::controls::values;
use crate::ComponentGallery;
use crate::samples::{Sample, flag, number, sample, sized, store_flag, store_number};

const SLIDER_WIDTH: f32 = 220.;
const STEPPER_WIDTH: f32 = 440.;
const SLIDER_MIN: f32 = 0.;
const SLIDER_MAX: f32 = 100.;
const SLIDER_STEP: f32 = 5.;
const SLIDER_VALUE: f32 = 40.;
const RATING_MAX: usize = 5;
const RATING_VALUE: usize = 3;
const STEPS: [&str; 3] = ["Details", "Review", "Done"];
const RADIO_CHOICES: [&str; 3] = ["Daily", "Weekly", "Monthly"];
const RADIO: &str = "kit-radio";
const RATING: &str = "kit-rating";
const STEP: &str = "kit-step";
const TOGGLE: &str = "kit-toggle";
const SWITCH: &str = "kit-switch";
const FORMAT: [(&str, &str); 3] = [
    ("kit-toggle-bold", "Bold"),
    ("kit-toggle-italic", "Italic"),
    ("kit-toggle-underline", "Underline"),
];

/// A retained slider of the `Slider` samples.
pub(crate) fn slider_state(cx: &mut App) -> Entity<SliderState> {
    cx.new(|_| {
        SliderState::new()
            .min(SLIDER_MIN)
            .max(SLIDER_MAX)
            .step(SLIDER_STEP)
            .default_value(SLIDER_VALUE)
    })
}

pub(crate) fn radios(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = values(gallery);
    let selected = number(&values, RADIO, 1, cx);
    let group = RadioGroup::horizontal("gallery-kit:radio-group")
        .selected_index(Some(selected))
        .children(RADIO_CHOICES.map(choice))
        .on_click(move |index, _, cx| store_number(&values, RADIO, *index, cx));
    vec![
        sample("Group", group),
        sample(
            "Disabled",
            Radio::new("gallery-kit:radio-disabled")
                .label("Unavailable")
                .disabled(true),
        ),
    ]
}

/// A radio whose ID and label are its fixture choice.
fn choice(label: &'static str) -> Radio {
    Radio::new(label).label(label)
}

pub(crate) fn ratings(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = values(gallery);
    let value = number(&values, RATING, RATING_VALUE, cx);
    vec![
        sample(
            "Interactive",
            Rating::new("gallery-kit:rating")
                .max(RATING_MAX)
                .value(value)
                .on_click(move |value, _, cx| store_number(&values, RATING, *value, cx)),
        ),
        sample(
            "Disabled",
            Rating::new("gallery-kit:rating-disabled")
                .max(RATING_MAX)
                .value(RATING_VALUE)
                .disabled(true),
        ),
    ]
}

pub(crate) fn sliders(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    // Each sample needs its own state: the thumb's ID comes from it, and a
    // repeated ID panics debug builds when accessibility is on.
    let basic = &gallery.retained.kit.basic;
    vec![
        sample(
            "Horizontal",
            sized(
                SLIDER_WIDTH,
                Slider::new(&basic.slider).accessibility_label("Fixture volume"),
            ),
        ),
        sample(
            "Disabled",
            sized(
                SLIDER_WIDTH,
                Slider::new(&basic.locked_slider)
                    .disabled(true)
                    .accessibility_label("Locked fixture volume"),
            ),
        ),
    ]
}

pub(crate) fn steppers(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = values(gallery);
    let step = number(&values, STEP, 1, cx);
    vec![sample(
        "Interactive; click a step",
        sized(
            STEPPER_WIDTH,
            Stepper::new("gallery-kit:stepper")
                .selected_index(step)
                .items(STEPS.map(|step| StepperItem::new().child(step)))
                .on_click(move |step, _, cx| store_number(&values, STEP, *step, cx)),
        ),
    )]
}

pub(crate) fn switches(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = values(gallery);
    let on = flag(&values, SWITCH, true, cx);
    vec![
        sample(
            "Interactive, through toggle::switch",
            toggle::switch("gallery-kit:switch", cx)
                .checked(on)
                .on_click(move |on, _, cx| store_flag(&values, SWITCH, *on, cx)),
        ),
        sample(
            "Disabled",
            toggle::switch("gallery-kit:switch-disabled", cx)
                .checked(true)
                .disabled(true),
        ),
    ]
}

pub(crate) fn toggles(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = values(gallery);
    let pressed = flag(&values, TOGGLE, false, cx);
    let single = values.clone();
    let toggle = Toggle::new("gallery-kit:toggle")
        .icon(IconName::Pin)
        .label("Pin")
        .checked(pressed)
        .on_click(move |pressed, _, cx| store_flag(&values, TOGGLE, *pressed, cx));
    let group = ToggleGroup::new("gallery-kit:toggle-group")
        .segmented()
        .children(FORMAT.map(|(key, label)| {
            Toggle::new(key)
                .label(label)
                .checked(flag(&single, key, false, cx))
        }))
        .on_click(move |checks, _, cx| {
            for ((key, _), checked) in FORMAT.iter().zip(checks) {
                store_flag(&single, key, *checked, cx);
            }
        });
    vec![
        sample("Interactive", toggle),
        sample("Segmented group", group),
    ]
}
