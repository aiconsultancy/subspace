//! GPUI Kit form components, in GPUI Kit's documentation order, with
//! InputGroup from its story gallery.

mod fields;
mod pickers;
#[cfg(test)]
mod tests;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::color_picker::ColorPickerState;
use gpui_kit::component::combobox::ComboboxState;
use gpui_kit::component::date_picker::DatePickerState;
use gpui_kit::component::input::{EditorState, InputState, OtpState, TextareaState};
use gpui_kit::component::select::{SearchableVec, SelectState};
use gpui_kit::component::speech::SpeechState;
use gpui_kit::component::time_field::TimeFieldState;
use gpui_kit::{App, AppContext as _, Entity, Window};

use super::kit_source;
use crate::catalog::{Item, Kit};

const FRUIT: [&str; 5] = ["Apple", "Banana", "Cherry", "Date", "Elderberry"];
const OTP_LENGTH: usize = 6;
const TEXTAREA_ROWS: usize = 3;
const SOURCE: &str =
    "fn main() {\n    let fixture = \"component gallery\";\n    println!(\"{fixture}\");\n}\n";

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "gpui_kit::component::input::Input",
        source: kit_source!("input/input.rs"),
        used_in: "Every text field, through fields::input and the shared composer and chat input.",
        samples: fields::inputs,
        kit: Some(Kit {
            paths: &["input::Input", "input::InputState"],
            used: true,
            prefer: Some("fields::input"),
        }),
    },
    Item {
        name: "gpui_kit::component::input::Textarea",
        source: kit_source!("input/textarea.rs"),
        used_in: "Multiline fields: the chat input, the composer, Report issue and the Issues editor.",
        samples: fields::textareas,
        kit: Some(Kit {
            paths: &["input::Textarea", "input::TextareaState"],
            used: true,
            prefer: Some("fields::textarea"),
        }),
    },
    Item {
        name: "gpui_kit::component::input::Editor",
        source: kit_source!("input/editor.rs"),
        used_in: "Code and JSON fields, through fields::code_editor.",
        samples: fields::editors,
        kit: Some(Kit {
            paths: &["input::Editor", "input::EditorState"],
            used: true,
            prefer: Some("fields::code_editor"),
        }),
    },
    Item {
        name: "gpui_kit::component::select::Select",
        source: kit_source!("select.rs"),
        used_in: "Pickers in Settings, the theme designer, Space Lab and the Web manager, always built with anchored::select.",
        samples: fields::selects,
        kit: Some(Kit {
            paths: &["select::Select", "select::SelectState"],
            used: true,
            prefer: Some("anchored::select"),
        }),
    },
    Item {
        name: "gpui_kit::component::combobox::Combobox",
        source: kit_source!("combobox.rs"),
        used_in: "Not used in Subspace yet. Use it for a long, searchable list of choices.",
        samples: fields::comboboxes,
        kit: Some(Kit {
            paths: &["combobox::Combobox", "combobox::ComboboxState"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::input::NumberInput",
        source: kit_source!("input/number_input.rs"),
        used_in: "Font sizes in Settings > Appearance.",
        samples: fields::number_inputs,
        kit: Some(Kit {
            paths: &["input::NumberInput"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::date_picker::DatePicker",
        source: kit_source!("time/date_picker.rs"),
        used_in: "Not used in Subspace yet.",
        samples: pickers::date_pickers,
        kit: Some(Kit {
            paths: &["date_picker::DatePicker", "date_picker::DatePickerState"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::time_field::TimeField",
        source: kit_source!("time/time_field.rs"),
        used_in: "Not used in Subspace yet.",
        samples: pickers::time_fields,
        kit: Some(Kit {
            paths: &["time_field::TimeField", "time_field::TimeFieldState"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::input::OtpInput",
        source: kit_source!("input/otp_input.rs"),
        used_in: "Not used in Subspace yet.",
        samples: fields::otp_inputs,
        kit: Some(Kit {
            paths: &["input::OtpInput", "input::OtpState"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::speech::SpeechButton",
        source: kit_source!("speech/button.rs"),
        used_in: "Not used in Subspace. Subspace builds GPUI Kit without its speech feature, so the button shows as unsupported and never opens the microphone.",
        samples: pickers::speech_buttons,
        kit: Some(Kit {
            paths: &["speech::SpeechButton", "speech::SpeechState"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::color_picker::ColorPicker",
        source: kit_source!("color_picker.rs"),
        used_in: "Through anchored::color_select, every colour in the theme designer.",
        samples: pickers::color_pickers,
        kit: Some(Kit {
            paths: &[
                "color_picker::ColorPicker",
                "color_picker::ColorSelect",
                "color_picker::ColorPickerState",
            ],
            used: true,
            prefer: Some("anchored::color_select"),
        }),
    },
    Item {
        name: "gpui_kit::component::form::v_form",
        source: kit_source!("form/form.rs"),
        used_in: "Not used in Subspace; forms stack shared fields::form_field rows.",
        samples: fields::forms,
        kit: Some(Kit {
            paths: &["form::v_form", "form::h_form", "form::field", "form::Form"],
            used: false,
            prefer: Some("fields::form_field"),
        }),
    },
    Item {
        name: "gpui_kit::component::input::InputGroup",
        source: kit_source!("input/group.rs"),
        used_in: "The chat input's editor and toolbar frame.",
        samples: fields::input_groups,
        kit: Some(Kit {
            paths: &["input::InputGroup"],
            used: true,
            prefer: None,
        }),
    },
];

/// Retained entities of the form samples.
pub(crate) struct State {
    pub(crate) input: Entity<InputState>,
    pub(crate) search: Entity<InputState>,
    pub(crate) masked: Entity<InputState>,
    pub(crate) disabled: Entity<InputState>,
    pub(crate) textarea: Entity<TextareaState>,
    pub(crate) editor: Entity<EditorState>,
    pub(crate) select: Entity<SelectState<SearchableVec<String>>>,
    pub(crate) combobox: Entity<ComboboxState<SearchableVec<String>>>,
    pub(crate) combobox_multiple: Entity<ComboboxState<SearchableVec<String>>>,
    pub(crate) number: Entity<InputState>,
    pub(crate) otp: Entity<OtpState>,
    pub(crate) group: Entity<InputState>,
    pub(crate) name: Entity<InputState>,
    pub(crate) email: Entity<InputState>,
    pub(crate) bio: Entity<TextareaState>,
    pub(crate) date: Entity<DatePickerState>,
    pub(crate) time: Entity<TimeFieldState>,
    pub(crate) color: Entity<ColorPickerState>,
    pub(crate) speech: Entity<SpeechState>,
}

impl State {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            input: input(window, cx, "Type a fixture name", ""),
            search: input(window, cx, "Search fixtures", ""),
            masked: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .default_value("not-a-real-secret")
            }),
            disabled: input(window, cx, "", "Read only"),
            textarea: textarea(window, cx, "A few lines of fixture text"),
            editor: cx.new(|cx| {
                EditorState::new(window, cx)
                    .language("rust")
                    .default_value(SOURCE)
            }),
            select: cx.new(|cx| SelectState::new(fruit(), None, window, cx)),
            combobox: combobox(window, cx, false),
            combobox_multiple: combobox(window, cx, true),
            number: input(window, cx, "", "3"),
            otp: cx.new(|cx| OtpState::new(OTP_LENGTH, window, cx).masked(true)),
            group: input(window, cx, "example", ""),
            name: input(window, cx, "Ada Example", ""),
            email: input(window, cx, "ada@example.com", ""),
            bio: textarea(window, cx, "A short fixture biography"),
            date: cx.new(|cx| DatePickerState::new(window, cx)),
            time: cx.new(|cx| TimeFieldState::new(window, cx)),
            color: colour_picker(window, cx),
            speech: cx.new(SpeechState::new),
        }
    }
}

fn fruit() -> SearchableVec<String> {
    SearchableVec::new(FRUIT.map(str::to_owned).to_vec())
}

fn input(
    window: &mut Window,
    cx: &mut App,
    placeholder: &'static str,
    value: &'static str,
) -> Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(value)
    })
}

fn textarea(window: &mut Window, cx: &mut App, placeholder: &'static str) -> Entity<TextareaState> {
    cx.new(|cx| {
        TextareaState::new(window, cx)
            .rows(TEXTAREA_ROWS)
            .placeholder(placeholder)
    })
}

fn combobox(
    window: &mut Window,
    cx: &mut App,
    multiple: bool,
) -> Entity<ComboboxState<SearchableVec<String>>> {
    cx.new(|cx| {
        ComboboxState::new(fruit(), Vec::new(), window, cx)
            .searchable(true)
            .multiple(multiple)
    })
}

/// Starts at the theme's primary colour, so the sample follows the theme.
fn colour_picker(window: &mut Window, cx: &mut App) -> Entity<ColorPickerState> {
    let primary = cx.theme().primary;
    cx.new(|cx| ColorPickerState::new(window, cx).default_value(primary))
}
