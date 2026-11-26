//! The AgentsView page uses the shell's Save and Cancel draft.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Window,
};
use serde_json::{Map, Value, json};
use subspace_framework::dashboard::{
    Tokens,
    fields::{self, InputRole},
    secret::{self, SecretActions, SecretField},
    settings::{BoardSettingsEditor, SettingsEditor},
};
use url::Url;

use super::connection::{ADDRESS, Connection, KEY};

const API_KEY: secret::SecretWords = secret::SecretWords {
    saved: "API key saved",
    clear: "Clear saved API key",
    undo: "Undo clear",
    clear_to_change: "Clear the saved API key to change it.",
    clear_on_save: "Save removes the saved API key or replaces it with the one you enter.",
};

pub(super) fn editor(
    saved: Map<String, Value>,
    enabled: bool,
    window: &mut Window,
    cx: &mut App,
) -> SettingsEditor {
    SettingsEditor::new(cx.new(|cx| Draft::new(saved, enabled, window, cx)))
}

struct Draft {
    saved: Option<Connection>,
    address: Entity<InputState>,
    key: Entity<InputState>,
    clear: bool,
    hint: bool,
    enabled: bool,
    _subscriptions: [Subscription; 2],
}

impl Draft {
    fn new(
        saved: Map<String, Value>,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let address = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("https://fleet.example.com");
            input.set_value(
                saved.get(ADDRESS).and_then(Value::as_str).unwrap_or(""),
                window,
                cx,
            );
            input
        });
        let key = cx.new(|cx| InputState::new(window, cx).masked(true));
        let subscriptions = [
            cx.subscribe(&address, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&key, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        Self {
            saved: Connection::from_saved(&saved).ok().flatten(),
            address,
            key,
            clear: false,
            hint: false,
            enabled,
            _subscriptions: subscriptions,
        }
    }

    fn saved_key(&self) -> bool {
        self.saved.as_ref().is_some_and(|saved| saved.key.is_some())
    }

    fn toggle_clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear = !self.clear;
        self.hint = false;
        self.key.update(cx, |key, cx| key.set_value("", window, cx));
        cx.notify();
    }
}

impl Render for Draft {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        v_flex()
            .gap_3()
            .child(fields::form_field("AgentsView address",
                fields::input(&self.address, InputRole::Configuration, "AgentsView address")
                    .disabled(!self.enabled),
            ))
            .child(secret::secret_field(
                SecretField {
                    id: "agentsview-key".into(),
                    label: "API key".into(),
                    input: &self.key,
                    clear: self.clear,
                    saved: self.saved_key(),
                    locked: !self.enabled,
                    hinted: self.hint,
                    words: &API_KEY,
                },
                SecretActions {
                    on_toggle: cx.listener(|draft, _, window, cx| draft.toggle_clear(window, cx)),
                    on_hint: cx.listener(|draft, _, _, cx| {
                        draft.hint = true;
                        cx.notify();
                    }),
                },
                &tokens,
            ))
            .child(fields::note(&tokens, "Use the address of your fleet’s AgentsView server. Leave the API key blank when the server uses an authenticated SSH tunnel and requires no key."))
    }
}

impl BoardSettingsEditor for Draft {
    fn snapshot(&self, cx: &App) -> Value {
        json!([
            self.address.read(cx).value().as_str(),
            self.clear,
            !self.key.read(cx).value().is_empty()
        ])
    }

    fn values(&self, cx: &App) -> Result<Map<String, Value>, String> {
        let address = self.address.read(cx).value();
        let typed = self.key.read(cx).value();
        let connection = Connection::parse(address.as_str(), typed.as_str())?;
        let kept = self.saved.as_ref().filter(|_| !self.clear);
        let key = kept_key(kept, connection.as_ref().map(|c| &c.address))?;
        Ok(Map::from_iter([
            (
                ADDRESS.into(),
                json!(connection.as_ref().map_or("", |c| c.address.as_str())),
            ),
            (
                KEY.into(),
                json!(
                    key.or_else(|| connection.as_ref()?.key.as_ref().map(|key| key.expose()))
                        .unwrap_or("")
                ),
            ),
        ]))
    }

    fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }
}

/// Changing the destination requires an explicit clear or replacement.
fn kept_key<'a>(
    saved: Option<&'a Connection>,
    next: Option<&Url>,
) -> Result<Option<&'a str>, &'static str> {
    let Some(saved) = saved.filter(|saved| saved.key.is_some()) else {
        return Ok(None);
    };
    if next != Some(&saved.address) {
        return Err("Clear or replace the saved API key to change the AgentsView address.");
    }
    Ok(saved.key.as_ref().map(|key| key.expose()))
}

#[cfg(test)]
mod tests {
    use super::{Connection, kept_key};

    #[gpui_kit::test]
    fn the_editor_masks_the_saved_key_and_saves_a_replacement(cx: &mut gpui_kit::TestAppContext) {
        use super::{BoardSettingsEditor as _, Draft};
        use gpui_kit::test::TestWindowExt as _;
        use serde_json::json;

        cx.update(gpui_kit::init);
        let saved = json!({"address": "https://fleet.example.com/", "apiKey": "example-key"});
        let (editor, cx) = cx.add_window_view(|window, cx| {
            Draft::new(saved.as_object().unwrap().clone(), true, window, cx)
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            editor.update(cx, |editor, cx| {
                assert!(editor.key.read(cx).value().is_empty());
                assert_eq!(editor.values(cx).unwrap()["apiKey"], "example-key");
                editor.toggle_clear(window, cx);
                editor
                    .key
                    .update(cx, |input, cx| input.set_value("replacement", window, cx));
                assert_eq!(editor.values(cx).unwrap()["apiKey"], "replacement");
                assert!(!editor.snapshot(cx).to_string().contains("replacement"));
            });
        });
    }

    #[test]
    fn an_existing_key_requires_the_same_destination() {
        let saved = Connection::parse("https://fleet.example.com", "example-key")
            .unwrap()
            .unwrap();
        let same = url::Url::parse("https://fleet.example.com/").unwrap();
        let other = url::Url::parse("https://other.example.com/").unwrap();
        assert_eq!(kept_key(Some(&saved), Some(&same)), Ok(Some("example-key")));
        assert!(kept_key(Some(&saved), Some(&other)).is_err());
        assert!(kept_key(Some(&saved), None).is_err());
        assert_eq!(kept_key(None, Some(&other)), Ok(None));
    }
}
