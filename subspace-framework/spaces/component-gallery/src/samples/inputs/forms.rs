//! Form helpers: labelled rows, headings, notes, validation messages, the
//! path input, the multiline field, saved secrets and the reveal button.

use std::path::PathBuf;

use gpui_kit::component::Disableable as _;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, ClickEvent, Context, Entity, MouseDownEvent, ParentElement as _, Window,
};
use subspace_dashboard::fields::{self, InputRole};
use subspace_dashboard::secret::{self, SecretActions, SecretField, SecretWords};
use subspace_dashboard::{Tokens, reveal};

use crate::ComponentGallery;
use crate::samples::{Sample, flag, sample, sized, store_flag};

const WIDTH: f32 = 320.;
const TEXTAREA_ROWS: usize = 3;
const CLEARED: &str = "secret-cleared";
const HINTED: &str = "secret-hinted";
/// An obviously fake file, shown only; the disabled sample opens no folder.
const FIXTURE_FILE: &str = "component-gallery-fixture.json";

/// The retained inputs of the form helper samples.
pub(crate) struct Forms {
    pub(crate) name: Entity<InputState>,
    pub(crate) path: Entity<InputState>,
    pub(crate) locked_path: Entity<InputState>,
    pub(crate) notes: Entity<TextareaState>,
    pub(crate) saved: Entity<InputState>,
    pub(crate) unsaved: Entity<InputState>,
}

impl Forms {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let input = |window: &mut Window, cx: &mut App, value: &'static str| {
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        Self {
            name: input(window, cx, "Sample fixture"),
            path: input(window, cx, "~/fixtures/component-gallery.json"),
            locked_path: input(window, cx, "~/fixtures/locked.json"),
            notes: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(TEXTAREA_ROWS)
                    .placeholder("Notes about the fixture")
            }),
            saved: cx.new(|cx| InputState::new(window, cx).masked(true)),
            unsaved: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("Paste a token")
            }),
        }
    }
}

pub(crate) fn form_fields(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let input = fields::input(
        &gallery.retained.forms.name,
        InputRole::Configuration,
        "Name",
    );
    vec![sample(
        "Label above its control",
        sized(WIDTH, fields::form_field("Name", input)),
    )]
}

pub(crate) fn headings(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Settings page group",
        fields::heading("Connection", cx),
    )]
}

pub(crate) fn notes(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Muted help under a control",
        sized(
            WIDTH,
            fields::note(tokens, "Changes take effect after a restart."),
        ),
    )]
}

pub(crate) fn field_errors(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Validation under its control",
        sized(
            WIDTH,
            fields::field_error(
                tokens,
                "gallery-sample:field-error",
                "Enter a port between 1 and 65535.",
            ),
        ),
    )]
}

pub(crate) fn path_inputs(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    // Each sample needs its own state: the input and Choose… button IDs come
    // from it, and a repeated ID panics debug builds when accessibility is on.
    let forms = &gallery.retained.forms;
    vec![
        sample(
            "Choose… opens the file picker",
            sized(
                WIDTH,
                fields::path_input(&forms.path, "Fixture file", "Choose a fixture file", false),
            ),
        ),
        sample(
            "Disabled",
            sized(
                WIDTH,
                fields::path_input(
                    &forms.locked_path,
                    "Locked fixture file",
                    "Choose a fixture file",
                    true,
                ),
            ),
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
        "Multiline value",
        sized(
            WIDTH,
            fields::textarea(&gallery.retained.forms.notes, "Fixture notes"),
        ),
    )]
}

/// One secret row: its ID and label, input, `[saved, cleared, hinted]` and
/// wording. The gallery's rows are never locked.
fn field<'a>(
    (id, label): (&'static str, &'static str),
    input: &'a Entity<InputState>,
    [saved, clear, hinted]: [bool; 3],
    words: &'static SecretWords,
) -> SecretField<'a> {
    SecretField {
        id: id.into(),
        label: label.into(),
        input,
        clear,
        saved,
        locked: false,
        hinted,
        words,
    }
}

pub(crate) fn secrets(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let forms = &gallery.retained.forms;
    let values = gallery.retained.values.clone();
    let cleared = flag(&values, CLEARED, false, cx);
    let hinted = flag(&values, HINTED, false, cx);
    let toggle = values.clone();
    let hint = values.clone();
    let saved = secret::secret_field(
        field(
            ("gallery-sample:secret-saved", "Token"),
            &forms.saved,
            [true, cleared, hinted],
            &secret::TOKEN,
        ),
        SecretActions {
            on_toggle: move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                store_flag(&toggle, CLEARED, !cleared, cx);
            },
            on_hint: move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                store_flag(&hint, HINTED, true, cx);
            },
        },
        tokens,
    );
    let unsaved = secret::secret_field(
        field(
            ("gallery-sample:secret-unsaved", "Password"),
            &forms.unsaved,
            [false, false, false],
            &secret::PASSWORD,
        ),
        SecretActions {
            on_toggle: |_: &ClickEvent, _: &mut Window, _: &mut App| {},
            on_hint: |_: &MouseDownEvent, _: &mut Window, _: &mut App| {},
        },
        tokens,
    );
    vec![
        sample("Saved; click Clear", sized(WIDTH, saved)),
        sample("Not saved", sized(WIDTH, unsaved)),
    ]
}

pub(crate) fn reveal_buttons(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Platform label; disabled so the gallery opens no folder",
        v_flex().child(
            reveal::button("gallery-sample:reveal", PathBuf::from(FIXTURE_FILE)).disabled(true),
        ),
    )]
}
