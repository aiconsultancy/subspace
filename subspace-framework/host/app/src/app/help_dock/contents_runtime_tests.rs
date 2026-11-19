use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, Pixels, ScrollDelta, TestAppContext,
    Window, WindowBounds, WindowOptions, point, px, size,
};

use super::{BoardHelp, HelpContent, HelpPanel};
use crate::app::help_dock::fixtures::{GUIDE, IMAGES};
use crate::app::help_dock::state::Side;
use subspace_dashboard::HelpImage;

#[path = "contents_runtime_tests/sizing.rs"]
mod sizing;

/// A guide long enough to scroll, with a duplicate H2 title.
fn long_guide(title: &str) -> &'static str {
    let paragraph = "Body text that wraps across the narrow Help panel. ".repeat(12);
    let body = format!("{paragraph}\n\n{paragraph}\n\n{paragraph}\n\n");
    let guide = format!(
        "# {title} guide\n{body}## Setup\n{body}## Views\n{body}### Detail\n{body}## Setup\n{body}"
    );
    Box::leak(guide.into_boxed_str())
}

fn content(id: &'static str, markdown: &'static str, images: &'static [HelpImage]) -> HelpContent {
    HelpContent {
        board: Some(BoardHelp {
            id,
            title: id.into(),
            markdown,
            images,
        }),
        ..HelpContent::default()
    }
}

fn open(cx: &mut TestAppContext, content: HelpContent) -> (AnyWindowHandle, Entity<HelpPanel>) {
    let (window, panel) = open_undrawn(cx, content);
    draw(cx, window);
    (window, panel)
}

/// Opens the panel without drawing, so nothing has been laid out or loaded.
fn open_undrawn(
    cx: &mut TestAppContext,
    content: HelpContent,
) -> (AnyWindowHandle, Entity<HelpPanel>) {
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(360.), px(600.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut panel = HelpPanel::new(Side::Right, false, window, cx);
                    panel.set_content(content, cx);
                    panel
                })
            },
        )
        .unwrap()
    })
}

fn draw(cx: &mut TestAppContext, window: AnyWindowHandle) {
    for _ in 0..3 {
        act(cx, window, |window, cx| window.render_frame(cx));
        cx.run_until_parked();
    }
}

fn act<R>(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    f: impl FnOnce(&mut Window, &mut App) -> R,
) -> R {
    cx.update_window(window, |_, window, cx| f(window, cx))
        .unwrap()
}

/// How far the section's top is from the scroll viewport's top.
fn gap_to_top(window: &Window, section: &str) -> f32 {
    let viewport = window.find("help-dock-scroll").bounds().top();
    let top = window.find(section.to_owned()).bounds().top();
    f32::from(top - viewport).abs()
}

#[gpui_kit::test]
fn clicking_an_entry_scrolls_its_heading_to_the_top_and_focuses_it(cx: &mut TestAppContext) {
    let (window, panel) = open(cx, content("test", long_guide("Test"), &[]));
    let before = act(cx, window, |window, _| {
        window.find("help-dock-scroll").bounds()
    });
    act(cx, window, |window, cx| {
        assert_eq!(
            window.find("help-contents-test-3").label(),
            Some("Go to Setup"),
            "the second Setup entry has its own target"
        );
        assert!(
            window.find("help-dock-back-to-contents").visible(),
            "a guide with contents shows Back to contents before any jump"
        );
        window.click("help-contents-test-3", cx);
    });
    draw(cx, window);
    act(cx, window, |window, _| {
        assert!(gap_to_top(window, "help-dock-section-test-3") < 1.);
        assert_eq!(
            window.find("help-dock-section-test-3").focused(),
            Some(true)
        );
        assert_eq!(
            window.find("help-dock-section-test-3").label(),
            Some("Setup")
        );
        let back = window.find("help-dock-back-to-contents").bounds();
        let viewport = window.find("help-dock-scroll").bounds();
        assert!(window.find("help-dock-back-to-contents").visible());
        assert!(
            back.bottom() <= viewport.top() && !back.intersects(&viewport),
            "Back to contents sits above the scroll region and covers no help text"
        );
        assert_eq!(viewport, before, "the scroll area keeps its size and place");
    });
    assert_eq!(panel.read_with(cx, |panel, _| panel.jump), Some(3));
}

#[gpui_kit::test]
fn enter_and_space_activate_entries_from_the_keyboard(cx: &mut TestAppContext) {
    let (window, panel) = open(cx, content("keys", long_guide("Keys"), &[]));
    for (key, tabs, section) in [("enter", 2, 1), ("space", 3, 2)] {
        act(cx, window, |window, cx| {
            let toggle = panel.read(cx).contents_focus.clone();
            toggle.focus(window, cx);
            for _ in 0..tabs {
                window.press("tab", cx);
            }
            let entry = format!("help-contents-keys-{section}");
            assert_eq!(window.find(entry).focused(), Some(true), "{key}");
            window.press(key, cx);
        });
        draw(cx, window);
        act(cx, window, |window, _| {
            let target = format!("help-dock-section-keys-{section}");
            assert!(gap_to_top(window, &target) < 1., "{key}");
            assert_eq!(window.find(target).focused(), Some(true), "{key}");
        });
        assert_eq!(panel.read_with(cx, |panel, _| panel.jump), Some(section));
    }
}

#[gpui_kit::test]
fn a_jump_stays_at_the_top_while_images_above_it_load_and_layout_changes(cx: &mut TestAppContext) {
    let (window, panel) = open_undrawn(cx, content("images", GUIDE, IMAGES));
    // All three host-owned rasters are above this heading.
    let section = panel
        .read_with(cx, |panel, _| {
            let headings = panel.document.headings();
            headings
                .iter()
                .find(|heading| heading.title == "After the images")
                .map(|heading| heading.section)
        })
        .unwrap();
    let target = format!("help-dock-section-images-{section}");
    act(cx, window, |window, cx| {
        panel.update(cx, |panel, cx| panel.jump_to(section, window, cx));
    });
    draw(cx, window);
    act(cx, window, |window, _| {
        assert!(gap_to_top(window, &target) < 1.)
    });
    panel.update(cx, |panel, cx| {
        panel.protected = true;
        cx.notify();
    });
    draw(cx, window);
    act(cx, window, |window, _| {
        assert!(
            gap_to_top(window, &target) < 1.,
            "new content above the section must not move it"
        );
    });
}

#[gpui_kit::test]
fn scrolling_releases_the_jump_and_back_to_contents_returns_to_the_top(cx: &mut TestAppContext) {
    let (window, panel) = open(cx, content("back", long_guide("Back"), &[]));
    act(cx, window, |window, cx| {
        window.click("help-contents-back-2", cx)
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        let delta = ScrollDelta::Pixels(point(px(0.), px(40.)));
        window.scroll("help-dock-scroll", delta, cx);
    });
    draw(cx, window);
    panel.read_with(cx, |panel, _| {
        assert_eq!(panel.jump, None);
        assert!(panel.scroll.offset().y < Pixels::ZERO);
    });
    act(cx, window, |window, cx| {
        assert!(window.find("help-dock-back-to-contents").visible());
        window.click("help-shortcuts-filter", cx);
        window.input("zzz", cx);
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        assert!(
            window.try_find("help-dock-back-to-contents").is_none(),
            "a shortcut filter hides the document and its Back to contents row"
        );
        window.press("secondary-a", cx);
        window.press("backspace", cx);
    });
    draw(cx, window);
    panel.update(cx, |panel, cx| {
        panel.contents_open = false;
        cx.notify();
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        window.click("help-dock-back-to-contents", cx);
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        let panel = panel.read(cx);
        assert_eq!(panel.scroll.offset().y, Pixels::ZERO);
        assert!(panel.contents_open);
        assert_eq!(window.find("help-contents-toggle").focused(), Some(true));
        assert_eq!(window.find("help-contents-toggle").expanded(), Some(true));
        assert!(window.find("help-dock-back-to-contents").visible());
    });
}

#[gpui_kit::test]
fn switching_boards_rebuilds_the_contents_and_resets_the_scroll(cx: &mut TestAppContext) {
    let (window, panel) = open(cx, content("first", long_guide("First"), &[]));
    act(cx, window, |window, cx| {
        window.click("help-contents-first-3", cx)
    });
    draw(cx, window);
    assert!(panel.read_with(cx, |panel, _| panel.scroll.offset().y < Pixels::ZERO));
    panel.update(cx, |panel, cx| {
        panel.set_content(content("second", "# Only\n## One\n", &[]), cx);
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        let panel = panel.read(cx);
        assert_eq!(panel.jump, None);
        assert_eq!(panel.scroll.offset().y, Pixels::ZERO);
        assert!(
            window.try_find("help-contents-toggle").is_none(),
            "two headings do not need contents"
        );
        assert!(window.try_find("help-dock-back-to-contents").is_none());
        assert!(window.try_find("help-contents-first-3").is_none());
        assert!(window.find("help-dock-section-second-1").visible());
    });
    panel.update(cx, |panel, cx| {
        panel.set_content(content("third", long_guide("Third"), &[]), cx);
    });
    draw(cx, window);
    act(cx, window, |window, _| {
        assert_eq!(
            window.find("help-contents-third-0").label(),
            Some("Go to Third guide")
        );
        assert_eq!(window.find("help-contents-toggle").expanded(), Some(true));
    });
}

#[gpui_kit::test]
fn back_to_contents_at_the_top_reopens_and_focuses_the_contents(cx: &mut TestAppContext) {
    let (window, panel) = open(cx, content("top", long_guide("Top"), &[]));
    panel.update(cx, |panel, cx| {
        panel.contents_open = false;
        cx.notify();
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        assert_eq!(window.find("help-contents-toggle").expanded(), Some(false));
        window.click("help-dock-back-to-contents", cx);
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        assert_eq!(panel.read(cx).scroll.offset().y, Pixels::ZERO);
        assert_eq!(window.find("help-contents-toggle").expanded(), Some(true));
        assert_eq!(window.find("help-contents-toggle").focused(), Some(true));
    });
}

#[path = "contents_runtime_tests/find.rs"]
mod find;
