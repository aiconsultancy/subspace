use crate::{
    icon_button::IconButtonSize,
    native_grid::{GridCell, GridSnapshot, NativeGrid},
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext, px, size};

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..6 {
        cx.update(|window, cx| window.render_frame(cx));
        cx.run_until_parked();
    }
}

fn item(window: &mut gpui_kit::Window, label: &str) -> usize {
    let menu = window.within("popup-menu");
    (0usize..32)
        .find(|index| {
            menu.try_find(*index)
                .is_some_and(|item| item.label() == Some(label))
        })
        .unwrap_or_else(|| panic!("missing popup item {label}"))
}

#[gpui_kit::test]
fn xl_grid_toolbar_stays_single_row_and_hidden_commands_remain_available(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        IconButtonSize::ExtraLarge.apply(cx);
    });
    let source = GridSnapshot::new(
        vec!["Name".into(), "Value".into()],
        vec![vec![GridCell::text("item"), GridCell::number(2.)]],
    )
    .unwrap();
    let (grid, cx) =
        cx.add_window_view(|window, cx| NativeGrid::new("grid", "Result", source, window, cx));
    for width in [2400., 320.] {
        cx.simulate_resize(size(px(width), px(600.)));
        settle(cx);
        cx.update(|window, _| {
            let toolbar = window.find("grid-toolbar").bounds();
            assert_eq!(toolbar.size.height, px(72.));
            assert!(toolbar.right() <= window.bounds().right());
            assert_eq!(window.find("grid-sort-0").bounds().size.height, px(20.));
        });
    }
    cx.update(|window, cx| {
        let trigger = window.find("overflow-toolbar-button").bounds();
        let toolbar = window.find("grid-toolbar").bounds();
        assert_eq!(trigger.size.height, px(72.));
        assert!(trigger.left() >= toolbar.left() && trigger.right() <= toolbar.right());
        assert!((trigger.center().y - toolbar.center().y).abs() <= px(1.));
        window.click("overflow-toolbar-button", cx);
        let export = item(window, "Export all as CSV…");
        assert!(window.within("popup-menu").try_find(export).is_some());
        let copy = item(window, "Copy all");
        window.within("popup-menu").click(copy, cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(grid.read(cx).status.as_ref(), "Copied");
        assert!(
            window
                .find("grid-status")
                .label()
                .unwrap()
                .contains("Copied")
        );
        window.click("overflow-toolbar-button", cx);
        let options = item(window, "Grid options");
        window.within("popup-menu").click(options, cx);
    });
    cx.update(|_, cx| assert!(grid.read(cx).options_open));
}
