//! Draft-only icon size choice. Save persists before publishing the live value.

use super::{
    Settings,
    form::{Choice, choice},
};
use gpui_kit::{App, Context, Window};
use subspace_dashboard::{
    icon_button::{IconButtonSize, PREFERENCE_KEY},
    prefs,
};

pub(super) struct Draft {
    pub(super) choice: Choice,
    initial: IconButtonSize,
}

impl Draft {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let initial = IconButtonSize::current(cx);
        let choice = choice(
            IconButtonSize::ALL
                .into_iter()
                .map(|size| size.label().to_owned())
                .collect(),
            Some(initial.label()),
            false,
            window,
            cx,
        );
        Self { choice, initial }
    }

    fn selected(&self, cx: &App) -> IconButtonSize {
        let selected = self.choice.read(cx).selected_value().map(String::as_str);
        IconButtonSize::ALL
            .into_iter()
            .find(|size| Some(size.label()) == selected)
            .unwrap_or_default()
    }

    pub(super) fn save(&mut self, cx: &mut App) -> Result<(), String> {
        let selected = self.selected(cx);
        if selected == self.initial {
            return Ok(());
        }
        let before = prefs::get(cx, PREFERENCE_KEY);
        prefs::set(cx, PREFERENCE_KEY, selected.key());
        if let Err(error) = prefs::save(cx) {
            match before {
                Some(value) => prefs::set(cx, PREFERENCE_KEY, value),
                None => prefs::remove(cx, PREFERENCE_KEY),
            }
            return Err(format!("Could not save icon button size: {error}"));
        }
        selected.apply(cx);
        self.initial = selected;
        Ok(())
    }
}
