//! Draft-only appearance choices that apply without a restart, such as the
//! icon button size. Save persists before publishing the live value.

use super::{
    Settings,
    form::{Choice, choice},
};
use gpui_kit::{App, Context, Window};
use subspace_dashboard::{
    icon_button::{IconButtonSize, PREFERENCE_KEY},
    prefs,
};

/// A finite appearance choice that Settings drafts, saves and applies live.
pub(super) trait Preference: Copy + Default + PartialEq + 'static {
    /// Its key in the state file.
    const KEY: &'static str;
    /// What it is, for a save error.
    const WHAT: &'static str;
    /// Every choice, in the order Settings lists them.
    fn all() -> Vec<Self>;
    fn current(cx: &App) -> Self;
    fn label(self) -> &'static str;
    fn key(self) -> &'static str;
    fn apply(self, cx: &mut App);
}

impl Preference for IconButtonSize {
    const KEY: &'static str = PREFERENCE_KEY;
    const WHAT: &'static str = "icon button size";
    fn all() -> Vec<Self> {
        Self::ALL.into()
    }
    fn current(cx: &App) -> Self {
        Self::current(cx)
    }
    fn label(self) -> &'static str {
        self.label()
    }
    fn key(self) -> &'static str {
        self.key()
    }
    fn apply(self, cx: &mut App) {
        self.apply(cx);
    }
}

pub(super) struct Draft<T: Preference> {
    pub(super) choice: Choice,
    initial: T,
}

impl<T: Preference> Draft<T> {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let initial = T::current(cx);
        let labels = T::all()
            .into_iter()
            .map(|choice| choice.label().to_owned())
            .collect();
        let choice = choice(labels, Some(initial.label()), false, window, cx);
        Self { choice, initial }
    }

    fn selected(&self, cx: &App) -> T {
        let selected = self.choice.read(cx).selected_value().map(String::as_str);
        T::all()
            .into_iter()
            .find(|choice| Some(choice.label()) == selected)
            .unwrap_or_default()
    }

    pub(super) fn save(&mut self, cx: &mut App) -> Result<(), String> {
        let selected = self.selected(cx);
        if selected == self.initial {
            return Ok(());
        }
        persist(T::KEY, selected.key(), T::WHAT, cx)?;
        selected.apply(cx);
        self.initial = selected;
        Ok(())
    }
}

/// Writes `value` under `key` before anything applies it, or restores what
/// was stored and says which choice (`what`) could not be saved.
fn persist(key: &str, value: &str, what: &str, cx: &mut App) -> Result<(), String> {
    let before = prefs::get(cx, key);
    prefs::set(cx, key, value);
    let Err(error) = prefs::save(cx) else {
        return Ok(());
    };
    match before {
        Some(value) => prefs::set(cx, key, value),
        None => prefs::remove(cx, key),
    }
    Err(format!("Could not save {what}: {error}"))
}
