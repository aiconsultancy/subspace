//! `toggle::switch` in each state on every surface the contrast rule covers.

use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex};
use gpui_kit::{App, Context, Hsla, ParentElement as _, Styled as _, Window, div};
use subspace_dashboard::toggle::{self, track};

use crate::ComponentGallery;
use crate::samples::{Sample, sample};
use subspace_dashboard::Tokens;

/// Readable names for the surfaces in [`track::surfaces`], in the same order.
const SURFACE_LABELS: [&str; 7] = [
    "Window",
    "Panel",
    "Secondary panel",
    "Sidebar",
    "Popover",
    "Selected row",
    "Active list row",
];

/// One switch state: its id suffix, checked, disabled and small.
const STATES: [(&str, bool, bool, bool); 6] = [
    ("off", false, false, false),
    ("on", true, false, false),
    ("disabled-off", false, true, false),
    ("disabled-on", true, true, false),
    ("small-off", false, false, true),
    ("small-on", true, false, true),
];

fn state_switch(surface: usize, state: (&str, bool, bool, bool), cx: &App) -> Switch {
    let (name, checked, disabled, small) = state;
    let id = format!("gallery-sample:switch-{surface}-{name}");
    let switch = toggle::switch(gpui_kit::SharedString::from(id), cx)
        .checked(checked)
        .disabled(disabled)
        .accessibility_label(name.replace('-', " "))
        .on_click(|_, _, _| {});
    if small { switch.small() } else { switch }
}

fn surface_row(surface: usize, background: Hsla, tokens: &Tokens, cx: &App) -> gpui_kit::Div {
    h_flex()
        .gap_3()
        .p_3()
        .rounded(tokens.surface_radius)
        .bg(background)
        .children(STATES.map(|state| div().child(state_switch(surface, state, cx))))
}

/// Off, on, disabled off, disabled on, small off and small on, on each surface.
pub(crate) fn switches(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let surfaces = track::surfaces(cx.theme());
    SURFACE_LABELS
        .into_iter()
        .zip(surfaces)
        .enumerate()
        .map(|(index, (label, (_, background)))| {
            sample(
                format!("{label}: off, on, disabled off, disabled on, small off, small on"),
                surface_row(index, background.into(), tokens, cx),
            )
        })
        .collect()
}
