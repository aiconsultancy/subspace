use std::path::{Path, PathBuf};

use gpui_kit::component::{Theme, try_parse_color};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, WindowHandle};
use subspace_core::state::State;
use subspace_dashboard::density::{self, Density};
use subspace_dashboard::prefs;

use super::Jig;

mod designer;
mod picker;
use crate::theme::{self, THEME_JSON, THEME_NAME, THEME_PREF, ThemeDocument};

const INITIAL_DENSITY: &str = r#"{"--row-h":"33px"}"#;

fn initialize(root: &Path, cx: &mut TestAppContext) -> PathBuf {
    let state_file = root.join("state.json");
    let mut state = State::default();
    state.set(THEME_PREF, "Solarized Light");
    state.set(density::STATE_KEY, INITIAL_DENSITY);
    state.save(&state_file).unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, state_file.clone());
        theme::initialize(cx);
    });
    state_file
}

fn open_jig(cx: &mut TestAppContext) -> WindowHandle<Jig> {
    let window = cx.add_window(Jig::new);
    window
        .update(cx, |jig, window, cx| jig.begin(window, cx))
        .unwrap();
    window
}

#[gpui_kit::test]
fn heading_switches_preview_without_persisting_and_respect_busy(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let state_file = initialize(root.path(), cx);
    let before_file = std::fs::read(&state_file).unwrap();
    let jig = open_jig(cx);
    let before = jig
        .update(cx, |jig, _, _| (jig.group_bold, jig.group_italic))
        .unwrap();
    open_section(jig, super::Section::Typography, cx);
    cx.update_window(jig.into(), |_, window, cx| {
        window.click("group-heading-bold", cx);
        window.click("group-heading-italic", cx);
    })
    .unwrap();
    jig.update(cx, |jig, _, cx| {
        assert_eq!((jig.group_bold, jig.group_italic), (!before.0, !before.1));
        jig.busy = true;
        cx.notify();
    })
    .unwrap();
    cx.update_window(jig.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("group-heading-bold", cx);
        window.click("group-heading-italic", cx);
    })
    .unwrap();
    jig.update(cx, |jig, _, _| {
        assert_eq!((jig.group_bold, jig.group_italic), (!before.0, !before.1));
    })
    .unwrap();
    assert_eq!(std::fs::read(state_file).unwrap(), before_file);
}

/// Open one designer section by its disclosure and let it finish revealing.
/// Reduced motion finishes the reveal at once, so the controls are hit-testable.
fn open_section(jig: WindowHandle<Jig>, section: super::Section, cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(jig.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(("trigger", section as usize), cx);
        // The section's last controls sit below the fold; scroll to them.
        let down = gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(-4000.));
        window.scroll(
            "theme-designer-scroll",
            gpui_kit::ScrollDelta::Pixels(down),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    jig.read_with(cx, |jig, _| assert!(jig.open.contains(&(section as usize))))
        .unwrap();
}

fn start_named_save(
    root: &Path,
    name: &str,
    color: &str,
    cx: &mut TestAppContext,
) -> (PathBuf, WindowHandle<Jig>) {
    let state_file = initialize(root, cx);
    let window = open_jig(cx);
    window
        .update(cx, |jig, window, cx| {
            jig.name
                .update(cx, |input, cx| input.set_value(name, window, cx));
            set_color(jig, "background", color, window, cx);
            jig.save(false, window, cx);
        })
        .unwrap();
    (state_file, window)
}

fn set_color(
    jig: &mut Jig,
    key: &str,
    value: &str,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<Jig>,
) {
    let picker = jig
        .colors
        .iter()
        .find(|field| field.key == key)
        .unwrap()
        .picker
        .clone();
    // An empty value clears the picker, as no colour chosen.
    picker.update(cx, |picker, cx| match try_parse_color(value) {
        Ok(color) => picker.set_value(color, window, cx),
        Err(_) => picker.clear_value(window, cx),
    });
    jig.preview(window, cx);
}

fn settle(cx: &TestAppContext) {
    cx.run_until_parked();
    cx.background_executor.run_until_parked();
}

fn assert_reset_to_base(
    jig: &mut Jig,
    base_name: &str,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<Jig>,
) {
    jig.reset(window, cx);
    let document = theme::document(base_name, cx).unwrap();
    let color = document.color("background").unwrap();
    let density = Density::from_json(&document.metadata().unwrap().density.to_string());
    assert_eq!(
        Theme::global(cx).colors.background,
        try_parse_color(&color).unwrap()
    );
    assert_eq!(cx.global::<Density>(), &density);
}

fn assert_export_only(jig: &mut Jig, cx: &mut gpui_kit::Context<Jig>) {
    jig.copy(cx);
    let json = cx.read_from_clipboard().unwrap().text().unwrap();
    let exported = ThemeDocument::parse(&json).unwrap();
    assert_eq!(exported.names(), ["My theme"]);
    assert_eq!(exported.color("background").as_deref(), Some("#123456"));
    assert_eq!(exported.metadata().unwrap().density["--row-h"], "41px");
    let group = exported.metadata().unwrap().group_heading;
    assert_eq!(group.size, 14.5);
    assert!(!group.bold);
    assert!(group.italic);
    assert_eq!(
        prefs::get(cx, THEME_PREF).as_deref(),
        Some("Solarized Light")
    );
}

#[gpui_kit::test]
fn cancel_restores_the_opening_appearance_and_reopen_starts_from_saved_preferences(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let state_file = initialize(root.path(), cx);
    let before_file = std::fs::read(&state_file).unwrap();
    let before = cx.update(|cx| {
        let theme = Theme::global(cx);
        (
            theme.colors,
            theme.font_family.clone(),
            theme.mono_font_family.clone(),
            cx.global::<Density>().clone(),
        )
    });
    let window = open_jig(cx);

    window
        .update(cx, |jig, window, cx| {
            set_color(jig, "background", "", window, cx);
            jig.save(false, window, cx);
            assert!(jig.status.to_string().contains("Page background"));
            assert!(!jig.busy);
            assert!(!root.path().join("themes/My theme.json").exists());
            jig.base.update(cx, |base, cx| {
                base.set_selected_value(&"Solarized Dark".to_owned(), window, cx)
            });
            jig.load_base(window, cx);
            set_color(jig, "background", "#123456", window, cx);
            cx.global_mut::<Density>().set("--row-h", 41.0);
            assert_eq!(
                Theme::global(cx).colors.background,
                try_parse_color("#123456").unwrap()
            );
            assert_reset_to_base(jig, "Solarized Dark", window, cx);
            set_color(jig, "background", "#123456", window, cx);
            cx.global_mut::<Density>().set("--row-h", 41.0);
            jig.group_size
                .update(cx, |slider, cx| slider.set_value(14.5, window, cx));
            jig.group_bold = false;
            jig.group_italic = true;
            jig.preview(window, cx);
            prefs::save(cx).unwrap();
            assert_export_only(jig, cx);
            jig.cancel(window, cx);
            assert_eq!(Theme::global(cx).colors, before.0);
            assert_eq!(Theme::global(cx).font_family, before.1);
            assert_eq!(Theme::global(cx).mono_font_family, before.2);
            assert_eq!(cx.global::<Density>(), &before.3);
            jig.begin(window, cx);
            assert_eq!(
                jig.base.read(cx).selected_value().map(String::as_str),
                Some("Solarized Light")
            );
        })
        .unwrap();

    assert_eq!(std::fs::read(&state_file).unwrap(), before_file);
    let state = State::load(&state_file);
    assert_eq!(state.get(THEME_PREF), Some("Solarized Light"));
    assert_eq!(state.get(density::STATE_KEY), Some(INITIAL_DENSITY));
}

#[gpui_kit::test]
fn save_updates_the_snapshot_so_later_preview_cancel_keeps_the_saved_theme(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (state_file, window) = start_named_save(root.path(), "Saved Theme", "#123456", cx);
    settle(cx);

    let path = root.path().join("themes/Saved Theme.json");
    let saved_text = std::fs::read_to_string(&path).unwrap();
    let saved = ThemeDocument::parse(&saved_text).unwrap();
    assert_eq!(saved.names(), ["Saved Theme"]);
    assert_eq!(saved.color("background").as_deref(), Some("#123456"));
    assert_eq!(saved.metadata().unwrap().density["--row-h"], "33px");
    assert_eq!(
        State::load(&state_file).get(THEME_PREF),
        Some("Saved Theme")
    );

    window
        .update(cx, |jig, window, cx| {
            set_color(jig, "background", "#654321", window, cx);
            assert_eq!(
                Theme::global(cx).colors.background,
                try_parse_color("#654321").unwrap()
            );
            jig.cancel(window, cx);
            assert_eq!(
                Theme::global(cx).colors.background,
                try_parse_color("#123456").unwrap()
            );
        })
        .unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), saved_text);
    assert_eq!(prefs_theme(cx).as_deref(), Some("Saved Theme"));
}

#[gpui_kit::test]
fn overwrite_confirmation_rejects_an_external_change_without_activating_the_draft(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("themes");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("Existing Theme.json");
    let existing = THEME_JSON.replace(THEME_NAME, "Existing Theme");
    std::fs::write(&path, &existing).unwrap();
    let (state_file, window) = start_named_save(root.path(), "Existing Theme", "#123456", cx);
    settle(cx);
    window
        .read_with(cx, |jig, _| assert!(jig.pending.is_some()))
        .unwrap();

    let external = format!("{existing}\n");
    std::fs::write(&path, &external).unwrap();
    window
        .update(cx, |jig, window, cx| jig.save(true, window, cx))
        .unwrap();
    settle(cx);

    assert_eq!(std::fs::read_to_string(path).unwrap(), external);
    window
        .read_with(cx, |jig, _| {
            assert!(jig.status.to_string().contains("has changed"))
        })
        .unwrap();
    assert_eq!(prefs_theme(cx).as_deref(), Some("Solarized Light"));
    assert_eq!(
        State::load(&state_file).get(THEME_PREF),
        Some("Solarized Light")
    );
}

fn prefs_theme(cx: &TestAppContext) -> Option<String> {
    cx.update(|cx| prefs::get(cx, THEME_PREF))
}
