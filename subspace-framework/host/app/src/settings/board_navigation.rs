use super::Settings;
use super::form::{Choice, choice};
use crate::app::navigation::{self, NavigationMode};
use gpui_kit::{Context, Window};
use subspace_dashboard::workspace::navigator_title;

pub(super) struct NavigationDraft {
    pub(super) mode: Choice,
    pub(super) collapsed: bool,
    /// Whether navigators show the Space's title.
    pub(super) navigator_title: bool,
    initial_mode: NavigationMode,
    initial_collapsed: bool,
    initial_navigator_title: bool,
}

impl NavigationDraft {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Settings>) -> Self {
        let initial_mode = navigation::read_mode(cx);
        let initial_collapsed = navigation::read_collapsed_preference(cx);
        let initial_navigator_title = navigator_title::shown(cx);
        let mode = choice(
            vec!["Horizontal tabs".to_owned(), "Vertical tabs".to_owned()],
            Some(mode_label(initial_mode)),
            false,
            window,
            cx,
        );
        Self {
            mode,
            collapsed: initial_collapsed,
            navigator_title: initial_navigator_title,
            initial_mode,
            initial_collapsed,
            initial_navigator_title,
        }
    }

    pub(super) fn save(&mut self, cx: &mut Context<Settings>) {
        let mode =
            self.mode
                .read(cx)
                .selected_value()
                .map_or(NavigationMode::Horizontal, |value| {
                    if value == "Vertical tabs" {
                        NavigationMode::Vertical
                    } else {
                        NavigationMode::Horizontal
                    }
                });
        if mode != self.initial_mode {
            navigation::save_mode(cx, mode);
            self.initial_mode = mode;
        }
        if self.collapsed != self.initial_collapsed {
            navigation::save_collapsed(cx, self.collapsed);
            self.initial_collapsed = self.collapsed;
        }
        if self.navigator_title != self.initial_navigator_title {
            navigator_title::set_shown(self.navigator_title, cx);
            self.initial_navigator_title = self.navigator_title;
        }
    }
}

fn mode_label(mode: NavigationMode) -> &'static str {
    match mode {
        NavigationMode::Horizontal => "Horizontal tabs",
        NavigationMode::Vertical => "Vertical tabs",
    }
}
