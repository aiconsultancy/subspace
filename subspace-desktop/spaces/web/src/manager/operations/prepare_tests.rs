use gpui_kit::TestAppContext;
use subspace_framework::core::settings::SettingsFile;
use subspace_framework::dashboard::prefs;

use super::Preparation;
use crate::install::ImportRequest;
use crate::manager::Manager;

#[gpui_kit::test]
fn preparing_without_application_settings_reports_it_and_is_not_busy(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(gpui_kit::init);
    cx.update(|cx| prefs::init_file(None, cx));
    let window = cx.add_window(|window, cx| Manager::new(window, cx, |_, _| Ok(())));
    window
        .update(cx, |manager, _, cx| {
            let preparation = Preparation {
                request: ImportRequest::Website {
                    url: "https://example.com/".into(),
                },
                id: "test-website".into(),
                title: "Test website".into(),
                replacing: false,
                file: SettingsFile::open(&root.path().join("config.json")).unwrap(),
            };
            manager.prepare(preparation, cx);
            assert!(!manager.busy);
            assert_eq!(
                manager.error.as_deref(),
                Some("Application settings are unavailable.")
            );
        })
        .unwrap();
}
