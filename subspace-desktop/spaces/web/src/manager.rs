//! Native collection settings, composed by the product rather than the shell.

mod editor;
mod model;
mod operations;
#[cfg(test)]
mod tests;
mod view;

use gpui_kit::{AnyView, App, AppContext as _, Context, Entity, Subscription, Window};
use subspace_framework::core::credentials::GITHUB_ACCOUNT_KEY;
use subspace_framework::core::settings::SettingsFile;
use subspace_framework::dashboard::{Services, prefs};

use crate::install::{PreparedImport, UpdateOffer};
use editor::{Editor, EditorSnapshot};
use model::Epoch;

/// The product applies changed configuration through its generic shell boundary.
pub type OnChanged = fn(&mut Window, &mut App) -> Result<(), String>;

/// Create a retained Settings extension. No imported code runs during setup.
// qual:api - the product constructs this native view through SettingsExtension.
pub fn create(window: &mut Window, cx: &mut App, changed: OnChanged) -> AnyView {
    cx.new(|cx| Manager::new(window, cx, changed)).into()
}

struct Manager {
    changed: OnChanged,
    installed: Vec<Installed>,
    editor: Option<Editor>,
    pending: Option<Pending>,
    offer: Option<OfferedUpdate>,
    remove: Option<String>,
    busy: bool,
    message: Option<String>,
    error: Option<String>,
    epoch: Epoch,
    subscriptions: Vec<Subscription>,
    release_subscription: Option<Subscription>,
    form_snapshot: Option<EditorSnapshot>,
}

struct Installed {
    id: String,
    title: String,
    source: String,
    version: Option<String>,
    tracked: bool,
}

struct Pending {
    id: String,
    title: String,
    file: SettingsFile,
    prepared: PreparedImport,
    ticket: u64,
    replacing: bool,
}

struct OfferedUpdate {
    id: String,
    title: String,
    offer: UpdateOffer,
    ticket: u64,
    file: SettingsFile,
}

impl Manager {
    fn new(_window: &mut Window, cx: &mut Context<Self>, changed: OnChanged) -> Self {
        let subscriptions = vec![cx.observe_global::<prefs::Prefs>(|this, cx| {
            this.sync_account(cx);
        })];
        Self {
            changed,
            installed: operations::installed(cx),
            editor: None,
            pending: None,
            offer: None,
            remove: None,
            busy: false,
            message: None,
            error: None,
            epoch: Epoch::new(selected_account(cx)),
            subscriptions,
            release_subscription: None,
            form_snapshot: None,
        }
    }

    fn sync_account(&mut self, cx: &mut Context<Self>) {
        if self.epoch.sync(selected_account(cx)) {
            self.pending = None;
            self.offer = None;
            self.busy = false;
            self.message = Some("GitHub account changed. Check or validate again with the current account. Installed boards are unchanged.".into());
            cx.notify();
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.epoch.invalidate();
        self.pending = None;
        self.offer = None;
        self.remove = None;
        self.busy = false;
        self.editor = None;
        self.release_subscription = None;
        self.form_snapshot = None;
        self.error = None;
        self.message = None;
        cx.notify();
    }

    fn fail(&mut self, error: String, cx: &mut Context<Self>) {
        self.busy = false;
        self.error = Some(error);
        cx.notify();
    }

    fn after_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(cx);
        match (self.changed)(window, cx) {
            Ok(()) => {
                self.installed = operations::installed(cx);
                self.message = Some("Applied immediately. No restart is needed.".into());
            }
            Err(error) => {
                self.error = Some(format!(
                    "Saved, but the running view could not refresh. {error}"
                ))
            }
        }
        cx.notify();
    }
}

fn selected_account(cx: &App) -> Option<String> {
    prefs::get(cx, GITHUB_ACCOUNT_KEY)
}

fn services(cx: &App) -> Result<&Services, String> {
    cx.try_global::<Services>()
        .ok_or_else(|| "Application settings are unavailable.".into())
}

fn config_path(cx: &App) -> Result<std::path::PathBuf, String> {
    services(cx).map(|services| services.config_file.clone())
}

fn new_input(
    text: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<gpui_kit::component::input::InputState> {
    cx.new(|cx| {
        let mut input = gpui_kit::component::input::InputState::new(window, cx);
        input.set_value(text.to_owned(), window, cx);
        input
    })
}
