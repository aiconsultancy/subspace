use super::{CONNECTION_NOTE, IssuesSettings};
use crate::{listing::Failure, scope::Access};
use gpui_kit::TestAppContext;
use subspace_framework::dashboard::settings::BoardSettingsEditor as _;
#[gpui_kit::test]
fn settings_have_no_inputs_or_saved_connection_choices(cx: &mut TestAppContext) {
    let mut settings = IssuesSettings::new();
    assert!(settings.connection_state().contains("up to a minute"));
    settings.access = Access::Ready {
        display_name: "Ada Fixture".into(),
    };
    assert_eq!(settings.connection_state(), "Signed in as Ada Fixture");
    settings.access = Access::Failed(Failure::Edge);
    assert_eq!(settings.connection_state(), "Sign-in required");
    settings.access = Access::Failed(Failure::Network);
    assert_eq!(settings.connection_state(), "Offline");
    assert_eq!(CONNECTION_NOTE, "Sign in under Settings › Account.");
    cx.update(|cx| {
        assert!(settings.values(cx).unwrap().is_empty());
        assert_eq!(settings.snapshot(cx), serde_json::Value::Null);
    });
}

#[gpui_kit::test]
fn settings_render_the_current_identity_without_a_sign_in_form(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|_, _| {
        let mut page = IssuesSettings::new();
        page.access = Access::Ready {
            display_name: "Ada Fixture".into(),
        };
        page
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("issues-connection-state").label(),
            Some("Signed in as Ada Fixture")
        );
        assert!(window.try_find("issues-organisation-button").is_none());
        assert!(window.try_find("issues-sign-in").is_none());
    });
}
