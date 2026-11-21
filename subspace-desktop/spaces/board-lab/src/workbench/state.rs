//! Cohesive state groups for [`super::Workbench`], split out so the root
//! struct groups fields by actual responsibility instead of holding every
//! field flat.

use std::collections::HashMap;

use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{SearchableVec, SelectState};
use gpui_kit::{App, Entity, SharedString, Task, Window};

use super::Workbench;

// The Space's views live with the shell that every view shares.
pub(super) use crate::shell::ViewTab;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResultTab {
    Chart,
    Data,
    Recipe,
}

impl ResultTab {
    /// In tab-bar order.
    pub(super) const ALL: [Self; 3] = [Self::Chart, Self::Data, Self::Recipe];

    /// The tab's position in the result tab bar.
    pub(super) fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|tab| *tab == self)
            .unwrap_or_default()
    }

    /// The tab at `index`; past the end, the last tab.
    pub(super) fn at(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::Recipe)
    }
}

#[cfg(test)]
mod result_tab_tests {
    use super::ResultTab;

    #[test]
    fn result_tabs_round_trip_through_their_index() {
        for (index, tab) in ResultTab::ALL.into_iter().enumerate() {
            assert_eq!(tab.index(), index);
            assert_eq!(ResultTab::at(index), tab);
        }
        assert_eq!(ResultTab::at(7), ResultTab::Recipe);
    }
}

/// Two alternative arrangements of the canvas, so we can compare them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CanvasLayout {
    Composer,
    Compact,
}

/// One of the preconfigured recipe families. `Typescript` is the optional
/// third profile: Bun is labelled honestly when the toolchain is unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Profile {
    Market,
    Services,
    Typescript,
}

impl Profile {
    pub(super) const ALL: [Self; 3] = [Self::Market, Self::Services, Self::Typescript];

    pub(super) fn key(self) -> &'static str {
        self.words().0
    }

    pub(super) fn title(self) -> &'static str {
        self.words().1
    }

    /// The profile's key and title, in one place per variant.
    // qual:allow(dry, repeated_matches) reason: "This enum has one mapping of its own; rustqual groups unrelated enums only because their three-arm variant matches share one shape."
    fn words(self) -> (&'static str, &'static str) {
        match self {
            Self::Market => ("market", "Market (Python)"),
            Self::Services => ("services", "Services (Node)"),
            Self::Typescript => ("typescript", "TypeScript (Bun)"),
        }
    }
}

/// Which tab, layout and profile the person is currently looking at, and
/// whether the navigator shows beside it.
pub(super) struct ViewState {
    pub(super) tab: ViewTab,
    pub(super) layout: CanvasLayout,
    pub(super) profile: Profile,
    pub(super) result_tab: ResultTab,
    pub(super) navigator: bool,
}

impl ViewState {
    /// `navigator` is the stored choice; it shows unless the person hid it.
    pub(super) fn new(navigator: bool) -> Self {
        Self {
            tab: ViewTab::Projects,
            layout: CanvasLayout::Composer,
            profile: Profile::Market,
            result_tab: ResultTab::Chart,
            navigator,
        }
    }
}

/// The draft canvas's input widgets: the title field, the environment pin
/// and package fields, and one field per bound parameter control.
pub(super) struct DraftInputs {
    pub(super) title_input: Entity<InputState>,
    pub(super) version_input: Entity<InputState>,
    pub(super) package_input: Entity<InputState>,
    pub(super) param_inputs: HashMap<&'static str, Entity<InputState>>,
    pub(super) param_selects: HashMap<&'static str, Entity<SelectState<SearchableVec<String>>>>,
}

impl DraftInputs {
    pub(super) fn new(title: &str, window: &mut Window, cx: &mut App) -> Self {
        Self {
            title_input: super::draft::text_input(title, window, cx),
            version_input: super::draft::text_input("", window, cx),
            package_input: super::draft::text_input("", window, cx),
            param_inputs: HashMap::new(),
            param_selects: HashMap::new(),
        }
    }
}

/// Background work tracking: whether a job is in flight, its handle, the
/// preset watcher, and the status line a person reads after an action.
pub(super) struct JobState {
    pub(super) busy: bool,
    pub(super) status: SharedString,
    pub(super) task: Option<Task<()>>,
    pub(super) watcher: Option<Task<()>>,
    pub(super) last_error: Option<String>,
}

impl JobState {
    pub(super) fn new(status: impl Into<SharedString>) -> Self {
        Self {
            busy: false,
            status: status.into(),
            task: None,
            watcher: None,
            last_error: None,
        }
    }
}

impl Workbench {
    pub(super) fn inputs_clean(&self, cx: &App) -> bool {
        self.inputs.title_input.read(cx).value().as_ref() == self.draft.title
            && self.inputs.param_inputs.iter().all(|(name, input)| {
                input.read(cx).value().as_ref()
                    == super::draft::parameter_value(&self.draft.parameters, name)
            })
            && self.inputs.param_selects.iter().all(|(name, input)| {
                input.read(cx).selected_value().is_some_and(|value| {
                    value == super::draft::parameter_value(&self.draft.parameters, name)
                })
            })
    }
}
