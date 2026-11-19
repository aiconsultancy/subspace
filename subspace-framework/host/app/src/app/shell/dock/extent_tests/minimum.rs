use super::{BOTTOM_DOCK_ID, MAIN_ID, PREFERRED_BOTTOM, PREFERRED_SIDE, SIDE_DOCK_ID, Shell};
use crate::app::terminal_fixture::{self, Panel};
use gpui_kit::base::PANEL_MIN_SIZE;
use gpui_kit::component::v_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Along as _, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, Size, Styled as _, TestAppContext, TestSupportExt as _,
    Window, div, px, size,
};
use subspace_dashboard::dock_panel::Placement;
use subspace_dashboard::icon_button::{IconButton, IconButtonSize};
use subspace_dashboard::prefs;

struct Content;

impl Render for Content {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("minimum-dock-content")
            .test_support()
            .size_full()
            .child(div().flex_1().min_h_0())
            .child(
                IconButton::new("minimum-dock-action")
                    .icon(subspace_lucide::IconName::RefreshCw)
                    .label("Dock action")
                    .accessibility_label("Dock action"),
            )
    }
}

fn assert_minimum(shell: &Shell, cx: &mut TestAppContext, minimum: Size<Pixels>) {
    cx.update_window(shell.window.into(), |_, window, _| {
        let content = window.find("minimum-dock-content").bounds();
        let action = window.find("minimum-dock-action").bounds();
        let main = window.find(MAIN_ID).bounds();
        assert!(content.size.width >= minimum.width && content.size.height >= minimum.height);
        assert!(action.top() >= content.top() && action.bottom() <= content.bottom());
        assert!(action.left() >= content.left() && action.right() <= content.right());
        assert!(main.size.width >= PANEL_MIN_SIZE && main.size.height >= PANEL_MIN_SIZE);
        assert!(
            content.bottom() <= window.viewport_size().height
                && content.right() <= window.viewport_size().width
        );
    })
    .unwrap();
}

fn assert_preferences(shell: &Shell, cx: &TestAppContext) {
    assert_eq!(shell.preferred(cx), (PREFERRED_BOTTOM, PREFERRED_SIDE));
    shell.app.read_with(cx, |_, cx| {
        assert_eq!(
            prefs::get(cx, "app.dock-bottom"),
            Some(PREFERRED_BOTTOM.to_string())
        );
        assert_eq!(
            prefs::get(cx, "app.dock-side"),
            Some(PREFERRED_SIDE.to_string())
        );
    });
}

fn intrinsic_shell(
    cx: &mut TestAppContext,
    placement: Placement,
) -> (Shell, Entity<Panel>, Size<Pixels>) {
    let shell = Shell::open(cx, placement, 1280., 1000.);
    let panel = shell
        .app
        .read_with(cx, |app, _| terminal_fixture::panel(app));
    let minimum = size(px(520.), px(360.));
    panel.update(cx, |panel, cx| {
        panel.minimum = Some(minimum);
        panel.content = Some(cx.new(|_| Content).into());
        cx.notify();
    });
    (shell, panel, minimum)
}

#[gpui_kit::test]
fn intrinsic_workspace_floor_wins_share_cap_without_changing_preferences(cx: &mut TestAppContext) {
    for placement in [Placement::Bottom, Placement::Left, Placement::Right] {
        let (shell, board, minimum) = intrinsic_shell(cx, placement);
        for choice in IconButtonSize::ALL {
            cx.update(|cx| choice.apply(cx));
            shell.resize(cx, 800., 700.);
            assert_minimum(&shell, cx, minimum);
            assert_preferences(&shell, cx);
        }
        board.update(cx, |board, cx| {
            board.minimum = None;
            cx.notify();
        });
        shell.resize(cx, 2000., 1000.);
        let (id, preferred) = if placement == Placement::Bottom {
            (BOTTOM_DOCK_ID, PREFERRED_BOTTOM)
        } else {
            (SIDE_DOCK_ID, PREFERRED_SIDE)
        };
        assert_eq!(shell.extents(cx, id).0, preferred);
    }
}

#[gpui_kit::test]
fn intrinsic_floor_and_native_main_region_define_the_supported_workspace_boundary(
    cx: &mut TestAppContext,
) {
    for placement in [Placement::Bottom, Placement::Left, Placement::Right] {
        let (shell, _, minimum) = intrinsic_shell(cx, placement);
        cx.update(|cx| IconButtonSize::ExtraLarge.apply(cx));
        shell.settle(cx);
        let bottom = placement == Placement::Bottom;
        let axis = if bottom {
            gpui_kit::Axis::Vertical
        } else {
            gpui_kit::Axis::Horizontal
        };
        let id = if bottom { BOTTOM_DOCK_ID } else { SIDE_DOCK_ID };
        let (dock, main) = shell.extents(cx, id);
        let outer = cx
            .update_window(shell.window.into(), |_, window, _| {
                f32::from(window.viewport_size().along(axis)) - dock - main
            })
            .unwrap();
        let intrinsic = minimum.along(axis);
        let boundary = f32::from(intrinsic + super::super::DOCK_BORDER + PANEL_MIN_SIZE) + outer;
        let (width, height) = if bottom {
            (1280., boundary)
        } else {
            (boundary, 1000.)
        };
        shell.resize(cx, width, height);
        assert_minimum(&shell, cx, minimum);
        let (width, height) = if bottom {
            (width, height - 1.)
        } else {
            (width - 1., height)
        };
        shell.resize(cx, width, height);
        let (dock, main) = shell.extents(cx, id);
        assert!(dock + main > boundary - outer - 1.);
        assert!(main >= f32::from(PANEL_MIN_SIZE));
        assert_preferences(&shell, cx);
    }
}
