//! Component naming and the report, without a window.

use gpui_kit::{Bounds, DivInspectorState, Edges, ElementId, StyleRefinement, point, px, size};

use super::super::model::{
    BoxModel, Component, compact_source, components, edges_label, element_id, named, shared_module,
    size_label,
};

fn name(text: &'static str) -> ElementId {
    ElementId::Name(text.into())
}

fn toolbar_path() -> Vec<ElementId> {
    vec![
        name("subspace_dashboard::surface_header::SurfaceHeader"),
        name("toolbar"),
        name("subspace_dashboard::icon_button::IconButton"),
        name("refresh"),
        name("gpui_component::button::button::Button"),
        name("subspace_dashboard::icon_button::content::Glyph"),
        name("subspace_lucide::SvgIcon"),
    ]
}

#[test]
fn the_innermost_shared_component_is_named_and_its_parts_are_not() {
    let path = toolbar_path();
    let found = components(&path);
    assert_eq!(found.len(), 5, "type scopes only, not plain ids");
    assert_eq!(
        named(&found).map(Component::name).as_deref(),
        Some("icon_button::IconButton")
    );
    assert_eq!(element_id(&path).as_deref(), Some("refresh"));
}

#[test]
fn without_a_shared_component_board_then_kit_components_are_named() {
    let board = [
        name("subspace_github_board::detail::Header"),
        name("gpui_component::button::button::Button"),
    ];
    assert_eq!(
        named(&components(&board)).map(Component::name).as_deref(),
        Some("subspace_github_board::detail::Header")
    );
    let kit = [name("gpui_component::switch::Switch")];
    assert_eq!(
        named(&components(&kit)).map(Component::name).as_deref(),
        Some("gpui_component::switch::Switch")
    );
    assert_eq!(
        Component {
            path: "gpui_component::button::button::Button".into()
        }
        .name(),
        "gpui_component::button::Button"
    );
}

#[test]
fn ordinary_ids_and_generic_arguments_are_not_mistaken_for_types() {
    let path = [
        name("settings-app-heading"),
        name("subspace_dashboard::list::Rows<alloc::string::String>"),
        name("a::b"),
        ElementId::Integer(3),
    ];
    let found = components(&path);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name(), "list::Rows");
    assert_eq!(element_id(&path).as_deref(), Some("3"));
}

#[test]
fn sources_are_compact_and_shared_helpers_name_their_module() {
    assert_eq!(
        compact_source(
            "/Users/me/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-component-0.9.1/src/switch.rs"
        ),
        "gpui-component-0.9.1/src/switch.rs"
    );
    let shared = "board-kit/dashboard/src/anchored/control.rs";
    assert_eq!(compact_source(shared), shared);
    assert_eq!(shared_module(shared).as_deref(), Some("anchored::control"));
    assert_eq!(
        shared_module("board-kit/dashboard/src/segmented.rs").as_deref(),
        Some("segmented")
    );
    assert_eq!(shared_module("host/app/src/app.rs"), None);
}

#[test]
fn sizes_and_edges_read_like_css_shorthand() {
    assert_eq!(size_label(size(px(28.), px(28.))), "28 × 28");
    assert_eq!(size_label(size(px(120.25), px(7.))), "120.3 × 7");
    let edges = |top: f32, right: f32, bottom: f32, left: f32| Edges {
        top: px(top),
        right: px(right),
        bottom: px(bottom),
        left: px(left),
    };
    assert_eq!(edges_label(&edges(4., 4., 4., 4.)), "4");
    assert_eq!(edges_label(&edges(4., 8., 4., 8.)), "4 8");
    assert_eq!(edges_label(&edges(4., 8., 2., 8.)), "4 8 2");
    assert_eq!(edges_label(&edges(1., 2., 3., 4.)), "1 2 3 4");
}

#[test]
fn the_box_model_resolves_rems_and_pixels() {
    let mut style = StyleRefinement::default();
    style.padding.left = Some(gpui_kit::rems(0.5).into());
    style.padding.top = Some(px(2.).into());
    style.border_widths.bottom = Some(px(1.).into());
    let state = DivInspectorState {
        base_style: Box::new(style),
        bounds: Bounds::new(point(px(0.), px(0.)), size(px(40.), px(20.))),
        content_size: size(px(30.), px(10.)),
    };
    let layout = BoxModel::new(&state, px(16.));
    assert_eq!(layout.padding.left, px(8.));
    assert_eq!(layout.padding.top, px(2.));
    assert_eq!(layout.border.bottom, px(1.));
    assert_eq!(layout.margin.top, px(0.));
}

#[test]
fn vendored_base_root_layers_are_skipped_just_like_registry_layers() {
    for file in [
        "vendor/gpui-base/src/root.rs",
        "/work/subspace-desktop/vendor/gpui-base/src/root.rs",
        "/registry/src/index/gpui-base-0.9.1/src/root.rs",
    ] {
        assert!(super::super::layers::is_root_container(file), "{file}");
    }
    assert!(!super::super::layers::is_root_container(
        "vendor/gpui-base-extra/src/root.rs"
    ));
    assert!(!super::super::layers::is_root_container(
        "framework/host/app/src/root.rs"
    ));
}
