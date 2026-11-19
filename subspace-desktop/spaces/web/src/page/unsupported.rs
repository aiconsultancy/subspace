use gpui_kit::{AnyView, App, Context, Window};

use super::{PageEvent, Profile, Start};

/// No page exists on this platform, so no value of this type does either.
pub(crate) enum Page {}

impl Page {
    pub(crate) fn open<T: 'static>(
        _start: &Start,
        _profile: &Profile,
        _window: &mut Window,
        _cx: &mut Context<T>,
    ) -> Result<Self, String> {
        Err("Web Spaces need macOS or Windows. Open the page in your browser instead.".into())
    }

    pub(crate) fn view(&self) -> AnyView {
        match *self {}
    }

    pub(crate) fn set_shown(&self, _shown: bool, _cx: &mut App) {
        match *self {}
    }

    pub(crate) fn take_events(&self) -> (Vec<PageEvent>, bool) {
        match *self {}
    }

    pub(crate) fn url(&self, _cx: &App) -> Option<String> {
        match *self {}
    }

    pub(crate) fn is_current(&self, _epoch: u64) -> bool {
        match *self {}
    }
    pub(crate) fn respond(&self, _epoch: u64, _script: &str, _cx: &App) {
        match *self {}
    }

    pub(crate) fn push(&self, _script: &str, _cx: &App) {
        match *self {}
    }

    pub(crate) fn back(&self, _cx: &App) -> Result<(), String> {
        match *self {}
    }

    pub(crate) fn forward(&self, _cx: &App) -> Result<(), String> {
        match *self {}
    }

    pub(crate) fn reload(&self, _cx: &App) -> Result<(), String> {
        match *self {}
    }

    pub(crate) fn load(&self, _url: &str, _cx: &App) -> Result<(), String> {
        match *self {}
    }
}
