//! Retained entities that stateful samples need. Each sample owns its own
//! entity, so no two samples share focus, bounds or popover state.

use gpui_kit::component::color_picker::ColorPickerState;
use gpui_kit::component::input::{EditorState, InputState};
use gpui_kit::component::select::{SearchableVec, SelectState};
use gpui_kit::{App, AppContext as _, Entity, Window};
use subspace_dashboard::native_grid::{GridCell, GridSnapshot, NativeGrid};
use subspace_dashboard::slider::{NumericSliderSpec, NumericSliderState};

use crate::samples::Values;
use crate::samples::inputs::forms::Forms;
use crate::samples::kit::{advanced, basic, form, layout};
use crate::samples::lists::fixture::FixtureList;
use crate::samples::messaging::Messaging;
use crate::samples::navigation;
use crate::samples::popover::PopoverHost;
use subspace_dashboard::Dashboard as _;
use subspace_dashboard::overflow_toolbar::OverflowToolbar;

pub(crate) type Choice = Entity<SelectState<SearchableVec<String>>>;

const SLIDER_MIN: f64 = 0.;
const SLIDER_MAX: f64 = 100.;
const SLIDER_STEP: f64 = 5.;
const SLIDER_VALUE: f64 = 40.;
const GRID_VALUES: [f64; 3] = [12., 7.5, 3.];
const CHOICES: [&str; 3] = ["Release", "Debug", "Profile"];
/// The `anchored::color_select` sample's starting colour.
const FIXTURE_COLOR: u32 = 0x0969da;
const EDITOR_TEXT: &str = "{\n  \"board\": \"component-gallery\",\n  \"fixture\": true\n}";

/// `fields::input` and `fields::labelled_input` states.
pub(crate) struct Inputs {
    pub(crate) search: Entity<InputState>,
    pub(crate) configuration: Entity<InputState>,
    pub(crate) compact: Entity<InputState>,
    pub(crate) disabled: Entity<InputState>,
    pub(crate) masked: Entity<InputState>,
    pub(crate) inline: Entity<InputState>,
    pub(crate) stacked: Entity<InputState>,
    pub(crate) error: Entity<InputState>,
}

/// `slider::control` states.
pub(crate) struct Sliders {
    pub(crate) value: Option<Entity<NumericSliderState>>,
    pub(crate) disabled: Option<Entity<NumericSliderState>>,
}

/// `anchored::select` states.
pub(crate) struct Selects {
    pub(crate) value: Choice,
    pub(crate) empty: Choice,
    pub(crate) small: Choice,
    pub(crate) disabled: Choice,
    pub(crate) popover: Choice,
    /// `anchored::color_select`.
    pub(crate) color: Entity<ColorPickerState>,
}

pub(crate) struct Retained {
    pub(crate) disclosure: subspace_dashboard::animated_collapsible::Disclosure,
    pub(crate) inputs: Inputs,
    pub(crate) editor: Entity<EditorState>,
    pub(crate) sliders: Sliders,
    pub(crate) selects: Selects,
    pub(crate) grid: Option<Entity<NativeGrid>>,
    /// Controlled values of the samples that keep no entity of their own.
    pub(crate) values: Entity<Values>,
    pub(crate) forms: Forms,
    pub(crate) messaging: Messaging,
    pub(crate) toolbar: Entity<OverflowToolbar>,
    pub(crate) popover: PopoverHost,
    pub(crate) list: Entity<FixtureList>,
    pub(crate) list_notes: Entity<FixtureList>,
    pub(crate) kit: KitStates,
    pub(crate) tab_dock: Entity<gpui_kit::component::dock::DockArea>,
}

/// Retained entities of the GPUI Kit samples, one set per group.
pub(crate) struct KitStates {
    pub(crate) basic: basic::State,
    pub(crate) form: form::State,
    pub(crate) layout: layout::State,
    pub(crate) advanced: advanced::State,
}

impl Retained {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            disclosure: Default::default(),
            inputs: Inputs {
                search: text(window, cx, "Search the board"),
                configuration: text(window, cx, "https://example.com/component-gallery"),
                compact: text(window, cx, "Compact"),
                disabled: text(window, cx, "Read only"),
                masked: secret(window, cx),
                inline: text(window, cx, "Inline value"),
                stacked: text(window, cx, "Stacked value"),
                error: text(window, cx, ""),
            },
            editor: editor(window, cx),
            sliders: Sliders {
                value: slider(window, cx, false),
                disabled: slider(window, cx, true),
            },
            selects: Selects {
                value: choice(window, cx, CHOICES.first().copied()),
                empty: choice(window, cx, None),
                small: choice(window, cx, CHOICES.get(1).copied()),
                disabled: choice(window, cx, CHOICES.first().copied()),
                popover: choice(window, cx, CHOICES.first().copied()),
                color: cx.new(|cx| {
                    ColorPickerState::new(window, cx).default_value(gpui_kit::rgb(FIXTURE_COLOR))
                }),
            },
            grid: grid(window, cx),
            values: cx.new(|_| Values::default()),
            forms: Forms::new(window, cx),
            messaging: Messaging::new(window, cx),
            toolbar: navigation::overflow_toolbar(cx),
            popover: PopoverHost::new(cx),
            list: cx.new(|cx| FixtureList::new(window, cx)),
            list_notes: cx.new(FixtureList::with_notes),
            tab_dock: crate::samples::choices::dock::fixture(window, cx),
            kit: KitStates {
                basic: basic::State::new(window, cx),
                form: form::State::new(window, cx),
                layout: layout::State::new(window, cx),
                advanced: advanced::State::new(window, cx),
            },
        }
    }
}

fn text(window: &mut Window, cx: &mut App, value: &'static str) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx).placeholder("Fixture text");
        state.set_value(value, window, cx);
        state
    })
}

/// A fixed, obviously fake value; the gallery never shows a real secret.
fn secret(window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx).masked(true);
        state.set_value("not-a-real-secret", window, cx);
        state
    })
}

fn editor(window: &mut Window, cx: &mut App) -> Entity<EditorState> {
    cx.new(|cx| {
        let mut state = EditorState::new(window, cx).language("json");
        state.set_value(EDITOR_TEXT, window, cx);
        state
    })
}

fn slider(window: &mut Window, cx: &mut App, disabled: bool) -> Option<Entity<NumericSliderState>> {
    let spec = NumericSliderSpec::new(SLIDER_MIN, SLIDER_MAX, SLIDER_STEP).ok()?;
    Some(cx.new(|cx| {
        let mut state = NumericSliderState::new(spec, SLIDER_VALUE, window, cx);
        state.set_disabled(disabled, window, cx);
        state
    }))
}

fn choice(window: &mut Window, cx: &mut App, selected: Option<&str>) -> Choice {
    let items = CHOICES.map(str::to_owned).to_vec();
    cx.new(|cx| {
        let mut state = SelectState::new(SearchableVec::new(items), None, window, cx);
        if let Some(selected) = selected {
            state.set_selected_value(&selected.to_owned(), window, cx);
        }
        state
    })
}

fn grid(window: &mut Window, cx: &mut App) -> Option<Entity<NativeGrid>> {
    let columns = ["Board", "Kind", "Count"].map(str::to_owned).to_vec();
    let rows = ["github", "actions", "irc"]
        .iter()
        .zip(GRID_VALUES)
        .map(|(board, count)| {
            vec![
                GridCell::text(*board),
                GridCell::text("fixture"),
                GridCell::number(count),
            ]
        })
        .collect();
    let snapshot = GridSnapshot::new(columns, rows).ok()?;
    Some(cx.new(|cx| {
        NativeGrid::new(
            "gallery-sample:native-grid",
            "Fixture grid",
            snapshot,
            window,
            cx,
        )
    }))
}
