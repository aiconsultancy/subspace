//! GPUI Kit pickers: date, time and colour, and the speech button.

use gpui_kit::component::Sizable as _;
use gpui_kit::component::color_picker::ColorPicker;
use gpui_kit::component::date_picker::DatePicker;
use gpui_kit::component::speech::SpeechButton;
use gpui_kit::component::time_field::TimeField;
use gpui_kit::{Context, Window};
use subspace_dashboard::Tokens;

use crate::ComponentGallery;
use crate::samples::{Sample, sample, sized};

const WIDTH: f32 = 260.;

pub(crate) fn date_pickers(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Click to open the calendar",
        sized(
            WIDTH,
            DatePicker::new(&gallery.retained.kit.form.date)
                .placeholder("Choose a date")
                .cleanable(true),
        ),
    )]
}

pub(crate) fn time_fields(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Hours and minutes; type or use arrow keys",
        TimeField::new(&gallery.retained.kit.form.time),
    )]
}

pub(crate) fn color_pickers(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Click the swatch",
        ColorPicker::new(&gallery.retained.kit.form.color).small(),
    )]
}

pub(crate) fn speech_buttons(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Unsupported in this build",
        SpeechButton::new(&gallery.retained.kit.form.speech)
            .small()
            .show_when_unsupported(true),
    )]
}
