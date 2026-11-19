//! Every density token, grouped as the theme designer shows them.

use super::{Control, ControlGroup, FONT_SIZES, choice, control};

/// Limits and steps shared by every button role: padding, text size and
/// corner radius, as (min, max, step) in pixels.
const HALF_STEP: f32 = 0.5;
const ROLE_PAD: (f32, f32, f32) = (0., 24., 0.25);
const ROLE_FONT: (f32, f32, f32) = (8., 18., 0.25);
const ROLE_RADIUS: (f32, f32, f32) = (0., 24., HALF_STEP);

/// Every token, grouped as the theme designer shows them. The first group's
/// sizes are also Settings' font sizes.
pub const GROUPS: &[ControlGroup] = &[
    ControlGroup {
        name: "Font sizes",
        controls: FONT_SIZES,
    },
    ControlGroup {
        name: "Rows",
        controls: &[
            control("--row-h", "Row height", (18., 48., 1.), "px", 28.),
            control("--pad-x", "Content inset", (0., 32., 1.), "px", 12.),
            control("--indent", "Indent", (0., 48., 1.), "px", 24.),
            control("--row-inset", "Selection inset", (0., 16., 1.), "px", 0.),
        ],
    },
    ControlGroup {
        name: "Boards",
        controls: &[
            control(
                "--lane-w",
                "Parent column width",
                (180., 960., 1.),
                "px",
                360.,
            ),
            control(
                "--done-w",
                "Done column width",
                (0.25, 1.5, 0.05),
                "fr",
                0.6,
            ),
            control(
                "--head-h",
                "Column header height",
                (18., 48., 1.),
                "px",
                28.,
            ),
            control("--tree-w", "File tree width", (160., 960., 1.), "px", 360.),
        ],
    },
    ControlGroup {
        name: "Window",
        controls: &[
            control("--chrome-h", "Title bar height", (24., 60., 1.), "px", 30.),
            control("--toolbar-h", "Toolbar height", (24., 60., 1.), "px", 36.),
            control(
                "--panel-w",
                "Detail panel width",
                (320., 1600., 1.),
                "px",
                600.,
            ),
            control("--motion", "Motion duration", (0., 400., 10.), "ms", 140.),
        ],
    },
    ControlGroup {
        name: "Shape",
        controls: &[
            control("--radius", "Control radius", (0., 12., 1.), "px", 4.),
            control(
                "--surface-radius",
                "Surface radius",
                (0., 20., 1.),
                "px",
                6.,
            ),
            // Navigation capsules: the corner radius as a fraction of half the
            // item's height, so 1 is a full capsule at every size.
            control("--nav-round", "Tab roundness", (0., 1., 0.05), "fr", 1.),
        ],
    },
    ControlGroup {
        name: "Toolbar buttons",
        controls: &button_role(
            [
                "--toolbar-button-h",
                "--toolbar-button-pad-x",
                "--toolbar-button-font",
                "--toolbar-button-radius",
            ],
            (16., 44.),
            [21., 7., 12.25, 6.],
        ),
    },
    ControlGroup {
        name: "Action buttons",
        controls: &button_role(
            [
                "--action-button-h",
                "--action-button-pad-x",
                "--action-button-font",
                "--action-button-radius",
            ],
            (16., 44.),
            [21., 7., 10.5, 6.],
        ),
    },
    ControlGroup {
        name: "Dialog buttons",
        controls: &button_role(
            [
                "--dialog-button-h",
                "--dialog-button-pad-x",
                "--dialog-button-font",
                "--dialog-button-radius",
            ],
            (16., 44.),
            [21., 7., 12.25, 6.],
        ),
    },
    ControlGroup {
        name: "Compact buttons",
        controls: &button_role(
            [
                "--compact-button-h",
                "--compact-button-pad-x",
                "--compact-button-font",
                "--compact-button-radius",
            ],
            (12., 32.),
            [17.5, 3.5, 10.5, 6.],
        ),
    },
    ControlGroup {
        name: "Dialogs and tabs",
        controls: &[
            control("--dialog-pad", "Dialog padding", (0., 32., 0.5), "px", 10.5),
            control("--dialog-gap", "Dialog spacing", (0., 24., 0.5), "px", 7.),
            control("--nav-h", "Board tab height", (20., 44., 1.), "px", 28.),
        ],
    },
    ControlGroup {
        name: "Control style",
        controls: &[
            control("--liquid-glass", "Liquid glass", (0., 1., 1.), "", 0.),
            choice(
                "--toolbar-button-rest",
                "Toolbar buttons at rest",
                FINISHES,
                (1., 0.),
            ),
            choice(
                "--toolbar-button-hover",
                "Toolbar buttons on hover",
                FINISHES,
                (1., 0.),
            ),
        ],
    },
];

/// A quiet toolbar button's finish, in [`crate::tokens::ButtonFinish`] order.
const FINISHES: &[(&str, &str)] = &[("flat", "Flat"), ("glass", "Glass")];

/// The indexes of the groups that hold control tokens: the button roles,
/// dialogs and tabs, and the control style.
pub const CONTROL_GROUPS: std::ops::Range<usize> = 5..11;

/// One button role's height, horizontal padding, text size and corner radius,
/// in pixels at [`super::REFERENCE_REM`].
const fn button_role(
    [h, pad_x, font, radius]: [&'static str; 4],
    (min_h, max_h): (f32, f32),
    [height, pad, size, corner]: [f32; 4],
) -> [Control; 4] {
    [
        control(h, "Height", (min_h, max_h, HALF_STEP), "px", height),
        control(pad_x, "Horizontal padding", ROLE_PAD, "px", pad),
        control(font, "Text size", ROLE_FONT, "px", size),
        control(radius, "Corner radius", ROLE_RADIUS, "px", corner),
    ]
}

#[cfg(test)]
mod tests {
    use super::button_role;

    #[test]
    fn a_button_role_has_four_tokens_with_shared_limits() {
        let [h, pad, font, radius] = button_role(
            ["--a-h", "--a-p", "--a-f", "--a-r"],
            (16., 44.),
            [21., 7., 12., 6.],
        );
        assert_eq!((h.token, h.min, h.max, h.default), ("--a-h", 16., 44., 21.));
        assert_eq!((pad.label, pad.default), ("Horizontal padding", 7.));
        assert_eq!((font.min, font.max), (8., 18.));
        assert_eq!((radius.token, radius.default), ("--a-r", 6.));
    }
}
