use super::{
    UpdatePanel,
    compact::CompactUpdates,
    updates::{ReleaseInfo, UpdateState},
    view::AboutUpdates,
};
use gpui_kit::component::{Theme, v_flex};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, px, size,
};
use subspace_dashboard::prefs;
use subspace_update::{Installed, UpdateProgress, Updater};

struct Previews {
    about: Entity<AboutUpdates>,
    compact: Entity<CompactUpdates>,
}

impl Render for Previews {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w(px(300.))
            .gap_4()
            .child(self.about.clone())
            .child(self.compact.clone())
    }
}

fn mount(cx: &mut TestAppContext) -> (Entity<UpdatePanel>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let product = subspace_product::fixture::products().unwrap().remove(0);
    let (previews, cx) = cx.add_window_view(move |_, cx| {
        let panel =
            cx.new(|_| UpdatePanel::new(Updater::for_product(&product, "1.0.0"), "Subspace"));
        let about = cx.new(|cx| AboutUpdates::new(panel.clone(), |_, _| {}, cx));
        let compact = cx.new(|cx| CompactUpdates::new(panel, |_, _| {}, cx));
        Previews { about, compact }
    });
    let panel = previews.read_with(cx, |view, cx| view.about.read(cx).panel.clone());
    cx.update(|window, cx| {
        window.replace_root(cx, |window, cx| {
            gpui_kit::base::Root::new(previews.clone(), window, cx)
        });
    });
    (panel, cx)
}

fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

#[gpui_kit::test]
fn compact_observes_the_same_busy_state_and_does_not_write_preferences(cx: &mut TestAppContext) {
    let (panel, cx) = mount(cx);
    panel.update(cx, |panel, cx| {
        assert!(panel.begin_check(cx).is_some());
    });
    settle(cx);
    cx.update(|window, cx| {
        let compact = window.within("settings-updates");
        assert_eq!(
            compact.find("update-status").label(),
            Some("Checking for updates…")
        );
        assert!(compact.find("settings-update-action").visible());
        window
            .within("settings-updates")
            .click("settings-update-action", cx);
    });
    panel.update(cx, |panel, cx| {
        assert_eq!(panel.state, UpdateState::Checking);
        panel.apply_check(Ok(subspace_update::CheckOutcome::UpToDate), cx);
        assert!(prefs::get(cx, crate::settings::CHECK_UPDATES_KEY).is_none());
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(
            window
                .within("settings-updates")
                .find("update-status")
                .label(),
            Some("Subspace v1.0.0 is up to date")
        );
    });
}

#[gpui_kit::test]
fn compact_and_about_copy_the_same_retained_error(cx: &mut TestAppContext) {
    let (panel, cx) = mount(cx);
    let seq = subspace_core::log::recent()
        .last()
        .map_or(0, |record| record.seq);
    panel.update(cx, |panel, cx| {
        panel.apply(super::updates::UpdateEvent::CheckRequested, cx);
        panel.apply_check(Err("Compact and About logging fixture".into()), cx);
    });
    settle(cx);
    cx.update(|window, cx| window.click("update-copy-details", cx));
    let about = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    cx.update(|window, cx| {
        assert_eq!(
            window.find("settings-update-copy-details").label(),
            Some("Copy details")
        );
        window.click("settings-update-copy-details", cx);
    });
    let compact = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert_eq!(about, compact);
    assert!(compact.contains("Compact and About logging fixture"));
    settle(cx);
    let records = subspace_core::log::recent_after(seq);
    let matching: Vec<_> = records
        .iter()
        .filter(|record| {
            record.target == "app"
                && record
                    .fields
                    .contains(&("message".into(), "Compact and About logging fixture".into()))
        })
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "both views share one accepted occurrence"
    );
    assert!(
        matching[0]
            .fields
            .contains(&("surface".into(), "update-error".into()))
    );
    assert!(matching[0].fields.contains(&("details".into(), about)));
}

#[gpui_kit::test]
fn compact_updates_from_available_through_progress_failure_and_restart(cx: &mut TestAppContext) {
    let (panel, cx) = mount(cx);
    for state in update_states() {
        panel.update(cx, |panel, cx| {
            panel.state = state;
            panel.progress = UpdateProgress::Downloading {
                received: 50,
                total: Some(100),
            };
            cx.notify();
        });
        settle(cx);
        cx.update(|window, cx| {
            let right = window.find("settings-updates").bounds().right();
            let compact = window.within("settings-updates");
            let status = compact.find("update-status");
            assert!(status.visible());
            assert!(status.bounds().right() <= right);
            let panel = panel.read(cx);
            let expected = match &panel.state {
                UpdateState::Available(_) => Some("Update to v1.2.0".to_owned()),
                UpdateState::Installed(_) => Some("Restart to use v1.2.0".to_owned()),
                _ => panel.status(),
            };
            assert_eq!(status.label(), expected.as_deref());
            assert!(compact.find("settings-update-action").visible());
            assert!(compact.try_find("check-for-updates").is_none());
            assert!(compact.try_find("install-update").is_none());
            assert_eq!(
                compact.try_find("update-download-progress").is_some(),
                matches!(panel.state, UpdateState::Installing(_))
            );
        });
    }
}

fn update_states() -> Vec<UpdateState> {
    let release = ReleaseInfo {
        version: "1.2.0".into(),
        html_url: "https://example.test/release".into(),
    };
    vec![
        UpdateState::Available(release.clone()),
        UpdateState::Installing(release),
        UpdateState::Failed("Could not verify update".into()),
        UpdateState::Installed(Installed {
            previous_version: "1.0.0".into(),
            version: "1.2.0".into(),
            path: "test-only-unused".into(),
        }),
    ]
}

#[gpui_kit::test]
fn production_sidebar_contains_every_update_state_at_narrow_width_and_larger_font(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("config.json"), "{}").unwrap();
    let (handle, _) =
        crate::app::toolbar::tests::status::rooted_fixture(cx, root.path(), 1000., false);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.simulate_window_resize(handle, size(px(640.), px(900.)));
    // Match the app binding: Linux reserves plain Ctrl for the terminal.
    let settings_key = if cfg!(target_os = "linux") {
        "ctrl-shift-,"
    } else {
        "secondary-,"
    };
    cx.update_window(handle, |_, window, cx| window.press(settings_key, cx))
        .unwrap();
    cx.run_until_parked();
    let panel = cx.update(|cx| super::panel(cx).unwrap());
    let states = [
        UpdateState::Idle,
        UpdateState::Checking,
        UpdateState::UpToDate,
        UpdateState::Failed("test-only-long-error-without-spaces-".repeat(12)),
        UpdateState::Failed("Could not check for updates. Fixture service unavailable.".into()),
        UpdateState::Available(ReleaseInfo {
            version: "1.2.0-test-only-long-release-name-".repeat(8),
            html_url: "https://example.test/release".into(),
        }),
    ];
    for state in states.into_iter().chain(update_states()) {
        panel.update(cx, |panel, cx| {
            panel.state = state;
            panel.progress = UpdateProgress::Downloading {
                received: 50,
                total: Some(100),
            };
            cx.notify();
        });
        for font_size in [16., 20.] {
            cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
            for width in [320., 640., 1000.] {
                cx.simulate_window_resize(handle, size(px(width), px(900.)));
                cx.update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    assert_update_containment(window);
                })
                .unwrap();
            }
        }
    }
}

fn assert_update_containment(window: &mut Window) {
    let sidebar = window.find("settings-navigation").bounds();
    let updates = window.find("settings-updates").bounds();
    let action = window.find("settings-update-action");
    assert!(action.visible());
    assert_eq!(updates.bottom(), sidebar.bottom() - window.rem_size() * 0.5);
    assert!(updates.left() >= sidebar.left());
    assert!(updates.right() <= sidebar.right());
    assert!(action.bounds().left() >= updates.left());
    assert!(action.bounds().right() <= updates.right());
    assert!(action.bounds().bottom() <= updates.bottom());
    assert_update_caption(window);
    if let Some(progress) = window.try_find("update-download-progress") {
        assert!(progress.visible());
        assert!(progress.bounds().left() >= updates.left());
        assert!(progress.bounds().right() <= updates.right());
        assert!(progress.bounds().bottom() <= updates.bottom());
    }
}

fn assert_update_caption(window: &mut Window) {
    let action = window.find("settings-update-action");
    let scope = window.within("settings-update-action");
    let glyph = scope.find("toolbar-icon").bounds();
    let label = scope.find("icon-button-label").bounds();
    assert!(
        glyph.left() >= action.bounds().left(),
        "glyph {glyph:?}, action {:?}",
        action.bounds()
    );
    assert!(
        label.left() >= glyph.right(),
        "label {label:?}, glyph {glyph:?}"
    );
    assert!(
        label.right() <= action.bounds().right(),
        "label {label:?}, action {:?}",
        action.bounds()
    );
    assert!(label.size.width > px(0.));
    assert_eq!(
        action.label(),
        window
            .within("settings-updates")
            .find("update-status")
            .label()
    );
}
