//! The Settings draft for the terminal's fonts; the host owns Save and Cancel.

use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::select::{SearchableVec, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::{IndexPath, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window,
};
use serde_json::{Map, Value, json};
use subspace_framework::dashboard::fields::{self, InputRole};
use subspace_framework::dashboard::settings::{BoardSettingsEditor, SettingsEditor};
use subspace_framework::dashboard::{Tokens, anchored, typography};

use super::{FAMILY, SIZE, parse_size, read};

const FAMILY_LABEL: &str = "Font family";
const SIZE_LABEL: &str = "Font size (pt)";
const AUTOMATIC: &str = "Automatic";

type Picker = SearchableVec<FamilyItem>;

struct FontEditor {
    family: Entity<SelectState<Picker>>,
    size: Entity<InputState>,
    enabled: bool,
    _subscriptions: [Subscription; 2],
}

pub(crate) fn factory(
    values: Map<String, Value>,
    enabled: bool,
    window: &mut Window,
    cx: &mut App,
) -> SettingsEditor {
    subspace_terminal::settings::register_platform_fonts();
    let fonts = subspace_terminal::settings::fixed_width_families(cx);
    SettingsEditor::new(cx.new(|cx| FontEditor::new(values, fonts, enabled, window, cx)))
}

#[derive(Clone)]
struct FamilyItem {
    family: Option<String>,
    title: SharedString,
}

impl SelectItem for FamilyItem {
    type Value = Option<String>;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.family
    }
}

fn family_picker(
    saved: Option<String>,
    names: Vec<String>,
    window: &mut Window,
    cx: &mut Context<SelectState<Picker>>,
) -> SelectState<Picker> {
    let installed = saved
        .as_deref()
        .and_then(|name| typography::installed_family(name, &names));
    let (items, selected) = family_items(saved, installed, names);
    SelectState::new(
        Picker::new(items),
        Some(IndexPath::default().row(selected)),
        window,
        cx,
    )
    .searchable(true)
}

fn family_items(
    saved: Option<String>,
    installed: Option<String>,
    names: Vec<String>,
) -> (Vec<FamilyItem>, usize) {
    let selected = installed.as_ref().or(saved.as_ref());
    let mut items = Vec::with_capacity(names.len() + 2);
    items.push(FamilyItem {
        family: None,
        title: AUTOMATIC.into(),
    });
    items.extend(names.into_iter().map(|name| FamilyItem {
        title: name.clone().into(),
        family: Some(name),
    }));
    if installed.is_none()
        && let Some(name) = &saved
    {
        items.push(FamilyItem {
            title: format!("{name} (unavailable)").into(),
            family: Some(name.clone()),
        });
    }
    let selected = items
        .iter()
        .position(|item| item.family.as_ref() == selected)
        .unwrap_or(0);
    (items, selected)
}

impl FontEditor {
    fn new(
        values: Map<String, Value>,
        names: Vec<String>,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let saved = read(&values);
        let family = cx.new(|cx| family_picker(saved.family, names, window, cx));
        let size = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(AUTOMATIC);
            if let Some(size) = saved.size {
                input.set_value(size.to_string(), window, cx);
            }
            input
        });
        let subscriptions = [
            cx.subscribe(&family, |_, _, _: &SelectEvent<Picker>, cx| cx.notify()),
            cx.subscribe(&size, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        Self {
            family,
            size,
            enabled,
            _subscriptions: subscriptions,
        }
    }

    fn family<'a>(&self, cx: &'a App) -> Option<&'a String> {
        self.family
            .read(cx)
            .selected_value()
            .and_then(Option::as_ref)
    }
}

impl BoardSettingsEditor for FontEditor {
    fn snapshot(&self, cx: &App) -> Value {
        json!({ FAMILY: self.family(cx), SIZE: self.size.read(cx).value().to_string() })
    }

    fn values(&self, cx: &App) -> Result<Map<String, Value>, String> {
        let size = parse_size(self.size.read(cx).value().as_ref())?;
        Ok(Map::from_iter([
            (FAMILY.to_owned(), json!(self.family(cx))),
            (SIZE.to_owned(), json!(size)),
        ]))
    }

    fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }
}

impl Render for FontEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let invalid = parse_size(self.size.read(cx).value().as_ref()).err();
        v_flex()
            .id("terminal-font-settings")
            .gap_3()
            .child(fields::heading("Font", cx))
            .child(fields::form_field(
                FAMILY_LABEL,
                anchored::select(&self.family)
                    .id("terminal-font-family")
                    .accessibility_label(FAMILY_LABEL)
                    .search_placeholder("Search fonts")
                    .disabled(!self.enabled),
            ))
            .child(fields::form_field(
                SIZE_LABEL,
                v_flex()
                    .gap_1()
                    .child(
                        fields::input(&self.size, InputRole::Configuration, SIZE_LABEL)
                            .disabled(!self.enabled),
                    )
                    .children(invalid.as_deref().map(|error| {
                        fields::field_error(&tokens, "terminal-font-size-error-text", error)
                            .id("terminal-font-size-error")
                            .test_support()
                    })),
            ))
            .child(fields::note(&tokens, automatic_help()))
    }
}

fn automatic_help() -> &'static str {
    if cfg!(target_os = "macos") {
        "Automatic uses Ghostty first. If its font family is unavailable, Subspace uses the default Terminal.app font and size. Save updates open sessions."
    } else {
        "Automatic uses Ghostty settings first, then Subspace Appearance. Save updates open sessions."
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{Focusable as _, TestAppContext};
    use serde_json::{Map, Value, json};
    use subspace_framework::dashboard::settings::BoardSettingsEditor as _;

    use super::{FAMILY, FontEditor, SIZE};

    #[gpui_kit::test]
    fn the_terminal_editor_selects_a_family_and_validates_typed_size(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            FontEditor::new(
                Map::new(),
                vec!["Fixture Mono".to_owned()],
                true,
                window,
                cx,
            )
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("terminal-font-family", cx);
        });
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();
        cx.update(|window, cx| {
            let editor = editor.read(cx);
            assert_eq!(editor.values(cx).expect("draft")[FAMILY], "Fixture Mono");
            editor.size.focus_handle(cx).focus(window, cx);
        });
        cx.simulate_input("100");
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(editor.read(cx).values(cx).is_err());
            assert!(window.try_find("terminal-font-size-error").is_some());
        });
        cx.simulate_keystrokes("backspace backspace backspace");
        cx.simulate_input("17.5");
        cx.update(|_, cx| {
            assert_eq!(
                editor.read(cx).values(cx).expect("draft")[SIZE],
                json!(17.5)
            );
        });
        cx.simulate_keystrokes("backspace backspace backspace backspace");
        cx.update(|_, cx| {
            assert_eq!(
                editor.read(cx).values(cx).expect("draft")[SIZE],
                Value::Null
            );
        });
    }
}
