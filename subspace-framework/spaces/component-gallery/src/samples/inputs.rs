//! Inputs: role-sized inputs, labelled inputs, the code editor and the
//! form helpers.

pub(crate) mod forms;

use gpui_kit::{Context, Window, px};
use subspace_dashboard::Tokens;
use subspace_dashboard::fields::{self, FieldLayout, InputRole, LabelledInput};

use super::{Sample, bounded, sample, sized};
use crate::ComponentGallery;
use crate::catalog::Item;

const FIELDS: &str = "board-kit/dashboard/src/fields.rs";
const INPUT_WIDTH: f32 = 240.;
const LABELLED_WIDTH: f32 = 320.;
const LABEL_WIDTH: f32 = 96.;
const EDITOR_SIZE: (f32, f32) = (320., 120.);

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "fields::input",
        source: FIELDS,
        used_in: "Native inputs sized by role in Space Lab, Web and the host.",
        samples: role_inputs,
        kit: None,
    },
    Item {
        name: "fields::labelled_input",
        source: FIELDS,
        used_in: "Labelled form fields in Actions and IRC dialogs, Space Lab and the host.",
        samples: labelled_inputs,
        kit: None,
    },
    Item {
        name: "fields::code_editor",
        source: FIELDS,
        used_in: "A code editor for structured text such as JSON; no board uses it at present.",
        samples: code_editor,
        kit: None,
    },
    Item {
        name: "fields::form_field",
        source: FIELDS,
        used_in: "Settings rows and board settings editors such as IRC: the label above its control.",
        samples: forms::form_fields,
        kit: None,
    },
    Item {
        name: "fields::heading",
        source: FIELDS,
        used_in: "Group headings inside Settings pages and the IRC network editor.",
        samples: forms::headings,
        kit: None,
    },
    Item {
        name: "fields::note",
        source: FIELDS,
        used_in: "Muted, selectable help under Settings controls.",
        samples: forms::notes,
        kit: None,
    },
    Item {
        name: "fields::field_error",
        source: FIELDS,
        used_in: "Validation messages under Settings and IRC editor fields.",
        samples: forms::field_errors,
        kit: None,
    },
    Item {
        name: "fields::path_input",
        source: FIELDS,
        used_in: "File paths with a Choose… button in the IRC network editor.",
        samples: forms::path_inputs,
        kit: None,
    },
    Item {
        name: "fields::textarea",
        source: FIELDS,
        used_in: "The Issues editor's multiline description.",
        samples: forms::textareas,
        kit: None,
    },
    Item {
        name: "secret::secret_field",
        source: "board-kit/dashboard/src/secret.rs",
        used_in: "Saved tokens in Settings and passwords in the IRC network editor; the saved value is never shown.",
        samples: forms::secrets,
        kit: None,
    },
    Item {
        name: "reveal::button",
        source: "board-kit/dashboard/src/reveal.rs",
        used_in: "Show in Finder for config.json in Settings > General and the Channels Space.",
        samples: forms::reveal_buttons,
        kit: None,
    },
];

pub(super) fn role_inputs(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let inputs = &gallery.retained.inputs;
    vec![
        sample(
            "Search",
            sized(
                INPUT_WIDTH,
                fields::input(&inputs.search, InputRole::Search, "Search sample"),
            ),
        ),
        sample(
            "Configuration",
            sized(
                INPUT_WIDTH,
                fields::input(
                    &inputs.configuration,
                    InputRole::Configuration,
                    "Configuration sample",
                ),
            ),
        ),
        sample(
            "Compact",
            sized(
                INPUT_WIDTH,
                fields::input(&inputs.compact, InputRole::Compact, "Compact sample"),
            ),
        ),
        sample(
            "Disabled",
            sized(
                INPUT_WIDTH,
                fields::input(
                    &inputs.disabled,
                    InputRole::Configuration,
                    "Disabled sample",
                )
                .disabled(true),
            ),
        ),
        sample(
            "Masked",
            sized(
                INPUT_WIDTH,
                fields::input(&inputs.masked, InputRole::Configuration, "Masked sample")
                    .mask_toggle(),
            ),
        ),
    ]
}

pub(super) fn labelled_inputs(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let inputs = &gallery.retained.inputs;
    let inline = FieldLayout::Inline {
        label_width: px(LABEL_WIDTH),
    };
    vec![
        sample(
            "Inline",
            sized(
                LABELLED_WIDTH,
                fields::labelled_input(
                    LabelledInput::new(&inputs.inline, "Inline label", inline),
                    tokens,
                ),
            ),
        ),
        sample(
            "Stacked",
            sized(
                LABELLED_WIDTH,
                fields::labelled_input(
                    LabelledInput::new(&inputs.stacked, "Stacked label", FieldLayout::Stacked),
                    tokens,
                ),
            ),
        ),
        sample(
            "Error",
            sized(
                LABELLED_WIDTH,
                fields::labelled_input(
                    LabelledInput::new(&inputs.error, "Required label", FieldLayout::Stacked)
                        .error(Some("Enter a value.")),
                    tokens,
                ),
            ),
        ),
    ]
}

pub(super) fn code_editor(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Default",
        bounded(
            EDITOR_SIZE,
            tokens,
            fields::code_editor(&gallery.retained.editor, "JSON sample"),
        ),
    )]
}
