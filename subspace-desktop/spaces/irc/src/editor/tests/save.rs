//! Save checks a switch to the account against the account signed in at
//! that moment, over the real hub, and forgets the switches a Save already
//! applied. Fictional accounts and servers only.

use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use subspace_framework::dashboard::settings::BoardSettingsEditor as _;

use super::super::{IrcSettings, account_view};
use crate::managed::tests::support::{Api, TestEnv, connect, fleets, member_of, settle};

/// Signed in as ada over a hub whose saved network connects to the aurora
/// fleet's server, with a Settings page open on the same settings and that
/// network switched to the account, not yet saved.
fn switched_over_a_hub(
    cx: &mut TestAppContext,
) -> (
    Entity<IrcSettings>,
    Entity<crate::hub::Hub>,
    Api,
    std::sync::Arc<TestEnv>,
    &mut VisualTestContext,
) {
    let mut manual = crate::fixtures::config();
    manual.port = 6697;
    manual.tls = true;
    let settings = crate::config::Settings {
        networks: vec![manual.clone()],
        ..crate::tests::settings()
    };
    let (_board, hub, cx) = crate::tests::mount(cx, settings);
    let api = Api::new(&["127.0.0.1"]);
    let env = connect(&hub, &api, cx);
    member_of(&hub, (Some(1), "user-ada"), fleets("127.0.0.1", None), cx);
    settle(&hub, cx);
    let raw = serde_json::json!({"networks": [manual]})
        .as_object()
        .cloned()
        .unwrap_or_default();
    let editor =
        cx.update(|window, cx| cx.new(|cx| IrcSettings::new(&raw, true, None, window, cx)));
    editor.update(cx, |editor, cx| {
        editor.ask_switch(Some(0), cx);
        account_view::switch(editor, 0, cx);
    });
    cx.update(|_, cx| assert!(editor.read(cx).networks.is_empty(), "switched"));
    (editor, hub, api, env, cx)
}

#[gpui_kit::test]
fn save_asks_the_account_signed_in_now_not_the_page_s_copy(cx: &mut TestAppContext) {
    let (editor, _hub, api, env, cx) = switched_over_a_hub(cx);
    // Signed out; neither the hub's timer nor the page has caught up.
    api.sign_in(401, serde_json::json!({}));
    *env.generation.lock().unwrap() = Some(2);
    cx.update(|_, cx| {
        let editor = editor.read(cx);
        assert!(editor.account.networks.person.is_some(), "the page's copy");
        let refused = editor.values(cx).unwrap_err();
        assert!(refused.contains("because you are signed out"), "{refused}");
    });
}

#[gpui_kit::test]
fn a_saved_switch_does_not_hold_back_a_later_save(cx: &mut TestAppContext) {
    let (editor, hub, api, env, cx) = switched_over_a_hub(cx);
    cx.update(|_, cx| assert!(editor.read(cx).values(cx).is_ok()));
    // The Save lands: the saved network is gone from the settings.
    hub.update(cx, |hub, cx| {
        let saved = crate::config::Settings {
            networks: Vec::new(),
            ..crate::tests::settings()
        };
        hub.apply(saved, cx);
    });
    cx.run_until_parked();
    api.sign_in(401, serde_json::json!({}));
    *env.generation.lock().unwrap() = Some(2);
    cx.update(|_, cx| {
        let values = editor.read(cx).values(cx);
        assert!(values.is_ok(), "{values:?}");
    });
}
