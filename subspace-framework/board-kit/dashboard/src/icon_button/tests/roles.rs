use super::IconButton;
use crate::action::{Intent, action_button, dialog_button};
use crate::density::Density;
use crate::icon_button::IconButtonSize;
use crate::icon_button::content::Geometry;
use crate::tokens::{ButtonRole, Controls, Tokens};
use gpui_kit::component::h_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, IntoElement, ParentElement as _, Pixels, Render, TestAppContext,
    Window, px, size,
};
use subspace_lucide::IconName;

/// The macOS values the Cupertino and Subspace One Dark themes set.
const MACOS: &str = r#"{
    "--toolbar-button-h": "24px", "--toolbar-button-pad-x": "10px",
    "--toolbar-button-font": "13px", "--toolbar-button-radius": "12px",
    "--action-button-h": "20px", "--action-button-pad-x": "8px",
    "--action-button-font": "11px", "--action-button-radius": "6px",
    "--dialog-button-h": "24px", "--dialog-button-pad-x": "12px",
    "--dialog-button-font": "13px", "--dialog-button-radius": "7px",
    "--compact-button-h": "16px", "--compact-button-pad-x": "4px",
    "--compact-button-font": "10px", "--compact-button-radius": "5px",
    "--liquid-glass": "1"
}"#;
/// GPUI Kit's button line height relative to the text size.
const LINE: f32 = 1.25;
/// Text lines snap to the device grid; the test window draws at 2x.
const SNAP: f32 = 0.5;

fn assert_line(label: Pixels, font: f32) {
    let expected = font * LINE;
    assert!(
        (f32::from(label) - expected).abs() <= SNAP,
        "{label:?} for {font}px text"
    );
}

fn assert_rems(actual: gpui_kit::Rems, pixels: f32) {
    assert!(
        (actual.0 - pixels / 14.).abs() < 1e-5,
        "{actual:?} is not {pixels}px"
    );
}

struct Roles;

impl Render for Roles {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .child(
                IconButton::new("toolbar")
                    .icon(IconName::Search)
                    .label("Find"),
            )
            .child(action_button("action", Intent::Neutral, None, "Retry"))
            .child(action_button(
                "action-icon",
                Intent::Accept,
                Some(IconName::Check),
                "Allow",
            ))
            .child(dialog_button(
                "dialog",
                Intent::Primary,
                Some(IconName::Save),
                "Save",
            ))
            .child(
                IconButton::new("compact")
                    .icon(IconName::X)
                    .label("Remove")
                    .compact(),
            )
    }
}

fn frame(window: &mut Window, id: &'static str) -> (Pixels, Pixels) {
    let button = window.find(id).bounds().size.height;
    let label = window
        .within(id)
        .find("icon-button-label")
        .bounds()
        .size
        .height;
    (button, label)
}

fn render_roles(density: Option<Density>, cx: &mut TestAppContext) -> Vec<(Pixels, Pixels)> {
    cx.update(gpui_kit::init);
    if let Some(density) = density {
        cx.update(|cx| cx.set_global(density));
    }
    let handle = cx.open_window(size(px(1200.), px(200.)), |_, _| Roles);
    cx.update_window(*handle, |_, window, cx| {
        window.set_rem_size(px(crate::density::REFERENCE_REM));
        window.render_frame(cx);
        ["toolbar", "action", "action-icon", "dialog", "compact"]
            .map(|id| frame(window, id))
            .to_vec()
    })
    .unwrap()
}

#[gpui_kit::test]
fn default_roles_keep_the_existing_geometry(cx: &mut TestAppContext) {
    let frames = render_roles(None, cx);
    let expected = [
        (21., 12.25),
        (21., 10.5),
        (21., 10.5),
        (21., 12.25),
        (17.5, 10.5),
    ];
    for ((height, label), (want_height, font)) in frames.into_iter().zip(expected) {
        assert_eq!(height, px(want_height));
        assert_line(label, font);
    }
}

#[gpui_kit::test]
fn macos_tokens_resize_every_role(cx: &mut TestAppContext) {
    let frames = render_roles(Some(Density::from_json(MACOS)), cx);
    let expected = [(24., 13.), (20., 11.), (20., 11.), (24., 13.), (16., 10.)];
    for ((height, label), (want_height, font)) in frames.into_iter().zip(expected) {
        assert_eq!(height, px(want_height));
        assert_line(label, font);
    }
}

#[test]
fn geometry_scales_the_nominal_size_and_caps_the_radius() {
    let defaults = Controls::from_density(&Density::default());
    let small = Geometry::resolve(
        ButtonRole::Toolbar,
        defaults.toolbar_button,
        IconButtonSize::Small,
    );
    let nominal = IconButtonSize::Small.metrics();
    assert_eq!(small.target, nominal.target());
    assert_eq!(small.glyph, nominal.glyph());
    assert_eq!(
        small.pad,
        gpui_kit::rems(IconButtonSize::Small.nominal().pad)
    );
    assert_eq!(small.radius, 6.);

    let macos = Controls::from_density(&Density::from_json(MACOS));
    let toolbar = Geometry::resolve(
        ButtonRole::Toolbar,
        macos.toolbar_button,
        IconButtonSize::Small,
    );
    assert_rems(toolbar.target, 24.);
    assert_rems(toolbar.font, 13.);
    assert_eq!(toolbar.radius, 12., "a 24px capsule");
    assert_eq!(toolbar.radius_at(px(14.)), px(12.));
    let large = Geometry::resolve(
        ButtonRole::Toolbar,
        macos.toolbar_button,
        IconButtonSize::Large,
    );
    assert_rems(large.target, 48.);
    assert_eq!(
        large.font, toolbar.font,
        "the global size does not zoom text"
    );
    let mut capsule = macos.toolbar_button;
    capsule.radius = px(40.);
    let capped = Geometry::resolve(ButtonRole::Toolbar, capsule, IconButtonSize::Small);
    assert_eq!(capped.radius, 12., "never more than half the height");
    let compact = Geometry::resolve(
        ButtonRole::Compact,
        macos.compact_button,
        IconButtonSize::Large,
    );
    assert_rems(compact.target, 16.);
}

#[test]
fn liquid_glass_is_a_theme_switch() {
    assert!(!Tokens::one_dark().controls.liquid_glass);
    assert!(Controls::from_density(&Density::from_json(MACOS)).liquid_glass);
}
