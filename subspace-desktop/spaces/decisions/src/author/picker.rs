//! Choice labels are for display. Requests take the selected record's key.

use gpui_kit::component::select::{SearchableVec, SelectItem, SelectState};
use gpui_kit::{App, AppContext as _, Context, Entity, SharedString, Window};

use crate::Decisions;

#[derive(Clone)]
pub(super) struct OptionEntry {
    label: SharedString,
    key: String,
}

impl SelectItem for OptionEntry {
    type Value = String;

    fn value(&self) -> &String {
        &self.key
    }

    fn title(&self) -> SharedString {
        self.label.clone()
    }
}

pub(super) struct Picker {
    pub(super) select: Entity<SelectState<SearchableVec<OptionEntry>>>,
}

impl Picker {
    pub(super) fn new(
        choices: Vec<(String, String)>,
        selected: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Decisions>,
    ) -> Self {
        let selected = selected
            .map(str::to_owned)
            .or_else(|| (choices.len() == 1).then(|| choices[0].1.clone()));
        let choices: Vec<_> = choices
            .into_iter()
            .map(|(label, key)| OptionEntry {
                label: label.into(),
                key,
            })
            .collect();
        let select = cx.new(|cx| {
            let mut state =
                SelectState::new(SearchableVec::new(choices), None, window, cx).searchable(true);
            if let Some(key) = &selected {
                state.set_selected_value(key, window, cx);
            }
            state
        });
        Self { select }
    }

    pub(super) fn value<'a>(&self, cx: &'a App) -> Option<&'a str> {
        self.select.read(cx).selected_value().map(String::as_str)
    }
}
