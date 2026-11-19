//! GPUI Kit text and choice fields: inputs, textareas, the code editor,
//! selects, comboboxes, number and one-time-code inputs, input groups and
//! form layout. Every state is retained in [`super::State`].

use gpui_kit::component::combobox::Combobox;
use gpui_kit::component::form::{field, h_form, v_form};
use gpui_kit::component::input::{
    Input, InputGroup, InputGroupAddon, InputGroupAddonAlignment, InputGroupText, NumberInput,
    OtpInput, Textarea,
};
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::{Context, ParentElement as _, Window};
use subspace_dashboard::{Tokens, anchored, fields};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{Sample, sample, sized};

const WIDTH: f32 = 260.;
const FORM_WIDTH: f32 = 420.;
const EDITOR_HEIGHT: f32 = 140.;

pub(crate) fn inputs(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = &gallery.retained.kit.form;
    vec![
        sample(
            "Placeholder, clean button",
            sized(WIDTH, Input::new(&state.input).cleanable(true)),
        ),
        sample(
            "Prefix icon",
            sized(
                WIDTH,
                Input::new(&state.search).prefix(Icon::from(IconName::Search).small()),
            ),
        ),
        sample(
            "Masked",
            sized(WIDTH, Input::new(&state.masked).mask_toggle()),
        ),
        sample(
            "Disabled",
            sized(WIDTH, Input::new(&state.disabled).disabled(true)),
        ),
    ]
}

pub(crate) fn textareas(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Fixed rows",
        sized(WIDTH, Textarea::new(&gallery.retained.kit.form.textarea)),
    )]
}

pub(crate) fn editors(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Rust, with line numbers",
        crate::samples::bounded(
            (FORM_WIDTH, EDITOR_HEIGHT),
            tokens,
            fields::code_editor(&gallery.retained.kit.form.editor, "Fixture Rust source"),
        ),
    )]
}

pub(crate) fn selects(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Through anchored::select",
        sized(
            WIDTH,
            anchored::select(&gallery.retained.kit.form.select)
                .placeholder("Choose a fruit")
                .accessibility_label("Fixture fruit"),
        ),
    )]
}

pub(crate) fn comboboxes(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = &gallery.retained.kit.form;
    vec![
        sample(
            "Searchable",
            sized(
                WIDTH,
                Combobox::new(&state.combobox)
                    .placeholder("Choose a fruit")
                    .search_placeholder("Search fruit…"),
            ),
        ),
        sample(
            "Multiple",
            sized(
                WIDTH,
                Combobox::new(&state.combobox_multiple).placeholder("Choose fruit"),
            ),
        ),
    ]
}

pub(crate) fn number_inputs(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Step buttons",
        sized(WIDTH, NumberInput::new(&gallery.retained.kit.form.number)),
    )]
}

pub(crate) fn otp_inputs(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Six digits, masked",
        OtpInput::new(&gallery.retained.kit.form.otp),
    )]
}

pub(crate) fn input_groups(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let group = InputGroup::new("gallery-kit:input-group")
        .addon(
            InputGroupAddon::new("gallery-kit:input-group-scheme")
                .child(InputGroupText::new().child("https://")),
        )
        .input(Input::new(&gallery.retained.kit.form.group))
        .addon(
            InputGroupAddon::new("gallery-kit:input-group-suffix")
                .align(InputGroupAddonAlignment::InlineEnd)
                .child(InputGroupText::new().child(".com")),
        );
    vec![sample("Text addons", sized(FORM_WIDTH, group))]
}

pub(crate) fn forms(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = &gallery.retained.kit.form;
    vec![
        sample(
            "Vertical",
            sized(
                FORM_WIDTH,
                v_form()
                    .child(field().label("Name").child(Input::new(&state.name)))
                    .child(
                        field()
                            .label("Email")
                            .required(true)
                            .description("We never send mail to fixtures.")
                            .child(Input::new(&state.email)),
                    ),
            ),
        ),
        sample(
            "Horizontal",
            sized(
                FORM_WIDTH,
                h_form().child(field().label("Bio").child(Textarea::new(&state.bio))),
            ),
        ),
    ]
}
