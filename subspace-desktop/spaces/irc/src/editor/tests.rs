//! The IRC Settings editor in a real window, with fictional networks.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Map, Value, json};
use subspace_framework::dashboard::settings::BoardSettingsEditor as _;

use super::{IrcSettings, Page};

mod account;
mod save;

fn saved() -> Map<String, Value> {
    json!({
        "networks": [
            {"id": "example", "name": "Example Network", "host": "irc.example.net",
             "port": 6697, "tls": true, "nickname": "fixture",
             "password": "server-secret",
             "sasl": {"username": "fixture", "password": "account-secret"},
             "channels": ["#subspace"], "futureKey": 7},
            {"id": "local", "name": "Acme IRC", "host": "127.0.0.1", "port": 16667,
             "tls": false, "nickname": "subspace"}
        ],
        "highlights": ["rocket"],
        "unrelated": {"keep": true}
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

fn mount(
    cx: &mut TestAppContext,
    saved: Map<String, Value>,
) -> (Entity<IrcSettings>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (editor, cx) = cx.add_window_view(|window, cx| {
        IrcSettings::new(
            &saved,
            true,
            Some("/home/fixture/.config/subspace/irc/config.json".into()),
            window,
            cx,
        )
    });
    cx.update(|window, cx| window.render_frame(cx));
    (editor, cx)
}

fn type_into(
    editor: &Entity<IrcSettings>,
    key: u64,
    pick: fn(&super::form::Inputs) -> &Entity<gpui_kit::component::input::InputState>,
    text: &str,
    cx: &mut VisualTestContext,
) {
    let text = text.to_owned();
    cx.update(|window, cx| {
        let input = pick(&editor.read(cx).networks[key as usize].inputs).clone();
        input.update(cx, |input, cx| input.set_value(text, window, cx));
        window.render_frame(cx);
    });
}

/// Done sits below the form, out of the small test window.
fn done(editor: &Entity<IrcSettings>, key: u64, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| editor.done(key, cx));
        window.render_frame(cx);
    });
}

#[gpui_kit::test]
fn the_page_says_where_message_history_is_kept_and_cleared(cx: &mut TestAppContext) {
    let (_editor, cx) = mount(cx, saved());
    cx.update(|window, _| {
        assert!(window.try_find("irc-history-settings").is_some());
    });
    assert!(super::view::HISTORY_NOTE.contains("Cache Channels"));
    assert!(super::view::HISTORY_NOTE.contains("Clear cache…"));
}

#[gpui_kit::test]
fn the_list_names_each_network_and_saves_edits_over_unknown_keys(cx: &mut TestAppContext) {
    let (editor, cx) = mount(cx, saved());
    cx.update(|window, _| {
        assert!(window.try_find("irc-settings").is_some());
        assert!(window.try_find(("irc-network", 1u64)).is_some());
    });
    let initial = cx.update(|_, cx| editor.read(cx).snapshot(cx));
    cx.update(|window, cx| window.click(("irc-edit-network", 0u64), cx));
    cx.update(|window, cx| window.render_frame(cx));
    assert_eq!(editor.read_with(cx, |editor, _| editor.page), Page::Edit(0));
    type_into(&editor, 0, |inputs| &inputs.host, "irc.example.org", cx);
    cx.update(|_, cx| {
        let editor = editor.read(cx);
        assert_ne!(editor.snapshot(cx), initial);
        let values = editor.values(cx).unwrap_or_default();
        let first = &values["networks"][0];
        assert_eq!(first["host"], "irc.example.org");
        assert_eq!(first["password"], "server-secret", "untouched secrets stay");
        assert_eq!(first["sasl"]["password"], "account-secret");
        assert_eq!(first["futureKey"], 7, "unknown network keys stay");
        assert_eq!(values["networks"][1]["id"], "local");
        assert_eq!(values["highlights"], json!(["rocket"]));
        assert!(
            values.get("unrelated").is_none(),
            "only owned keys are written"
        );
    });
}

#[gpui_kit::test]
fn saved_passwords_never_enter_an_input(cx: &mut TestAppContext) {
    let (editor, cx) = mount(cx, saved());
    cx.update(|_, cx| {
        let network = &editor.read(cx).networks[0];
        assert!(network.server_password.saved && network.sasl_password.saved);
        for secret in [&network.server_password, &network.sasl_password] {
            assert_eq!(secret.input.read(cx).value().as_ref(), "");
        }
        for input in network.inputs.all() {
            let text = input.read(cx).value().to_string();
            assert!(!text.contains("secret"), "{text}");
        }
    });
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| editor.toggle_clear(0, false, window, cx));
    });
    cx.update(|_, cx| {
        let values = editor.read(cx).values(cx).unwrap_or_default();
        assert!(
            values["networks"][0].get("password").is_none(),
            "Clear removes it"
        );
        assert_eq!(values["networks"][0]["sasl"]["password"], "account-secret");
    });
}

#[gpui_kit::test]
fn a_new_network_gets_a_unique_id_from_its_name(cx: &mut TestAppContext) {
    let (editor, cx) = mount(cx, saved());
    cx.update(|window, cx| window.click("irc-add-network", cx));
    assert_eq!(editor.read_with(cx, |editor, _| editor.page), Page::Edit(2));
    type_into(&editor, 2, |inputs| &inputs.name, "Example network", cx);
    type_into(&editor, 2, |inputs| &inputs.host, "irc.example.com", cx);
    cx.update(|_, cx| {
        let error = editor.read(cx).values(cx).err().unwrap_or_default();
        assert_eq!(error, "Example network: Enter a nickname.");
    });
    done(&editor, 2, cx);
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.page),
        Page::Edit(2),
        "Done stays while a value is missing"
    );
    cx.update(|_, cx| {
        let editor = editor.read(cx);
        assert!(editor.networks[2].checked, "missing values now show");
        let fields: Vec<_> = editor
            .problems(2, cx)
            .iter()
            .map(|problem| problem.field)
            .collect();
        assert_eq!(fields, [super::validate::Field::Nickname]);
    });
    type_into(&editor, 2, |inputs| &inputs.nickname, "fixture", cx);
    type_into(&editor, 2, |inputs| &inputs.name, "Example Network", cx);
    cx.update(|_, cx| {
        let values = editor.read(cx).values(cx).unwrap_or_default();
        assert_eq!(values["networks"][2]["id"], "example-network");
    });
    done(&editor, 2, cx);
    assert_eq!(editor.read_with(cx, |editor, _| editor.page), Page::List);
}

#[gpui_kit::test]
fn a_blank_new_network_is_dropped_on_done(cx: &mut TestAppContext) {
    let (editor, cx) = mount(cx, Map::new());
    cx.update(|window, _| assert!(window.try_find("irc-settings").is_some()));
    let initial = cx.update(|_, cx| editor.read(cx).snapshot(cx));
    cx.update(|window, cx| window.click("irc-add-network", cx));
    cx.update(|window, cx| window.render_frame(cx));
    done(&editor, 0, cx);
    cx.update(|_, cx| {
        let editor = editor.read(cx);
        assert!(editor.networks.is_empty());
        assert_eq!(editor.snapshot(cx), initial);
    });
}

#[gpui_kit::test]
fn removing_asks_with_the_network_name(cx: &mut TestAppContext) {
    let (editor, cx) = mount(cx, saved());
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| editor.ask_remove(Some(1), cx));
        window.render_frame(cx);
    });
    cx.update(|window, _| {
        assert!(window.try_find(("irc-remove-confirmation", 1u64)).is_some());
    });
    cx.update(|window, cx| window.click(("irc-remove-confirm", 1u64), cx));
    cx.update(|_, cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.networks.len(), 1);
        let values = editor.values(cx).unwrap_or_default();
        assert_eq!(values["networks"].as_array().map(Vec::len), Some(1));
    });
}

#[test]
fn the_confirmation_names_the_network() {
    assert_eq!(super::view::removal_title("Acme IRC"), "Remove “Acme IRC”?");
}
