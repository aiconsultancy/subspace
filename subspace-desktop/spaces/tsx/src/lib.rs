//! TSX Spaces (experimental): native Spaces written in TSX with nsh,
//! shipped as signed `.nshpkg` packages and run in the content pane. The
//! workspace shell around them stays GPUI Kit. A package reads the system
//! only through the capabilities its user granted in Settings.

mod fields;
mod grant;
mod host;
mod manage;
mod run;
mod saved;
mod store;
mod theme;
mod view;

#[cfg(test)]
mod fixtures;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui_kit::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, Render, SharedString,
    Window,
};
use subspace_framework::dashboard::settings::BoardSettings;
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{BoardArtwork, Dashboard, DashboardEvent, prefs, services};

pub use fields::settings_fields;

use crate::run::Running;
use crate::saved::Saved;
use crate::store::{Installed, Paths};
use subspace_tsx_system::Tier;

const NAVIGATOR_KEY: &str = "tsx.navigator";

/// What the content pane shows for the selected package.
pub(crate) enum Content {
    Loading,
    Empty,
    Refused(String),
    NeedsGrant(Vec<subspace_tsx_system::Tier>),
    LaunchFailed(String),
    Failed(String),
    Running,
}

/// The TSX Spaces Space.
pub struct TsxSpaces {
    focus: FocusHandle,
    config_file: Option<PathBuf>,
    saved: Saved,
    /// `None` while the packages are read and verified.
    packages: Option<Vec<Installed>>,
    selected: Option<PathBuf>,
    running: Option<Running>,
    /// Why the last install, removal or build record failed.
    notice: Option<String>,
    /// Why the selected package could not start.
    launch_error: Option<String>,
    /// The latest refresh: results of earlier ones are dropped.
    generation: u64,
    /// The latest refresh, for the background reads: only its inventory is
    /// written for Settings.
    latest: Arc<AtomicU64>,
    navigator: bool,
    visible: bool,
}

impl TsxSpaces {
    fn paths(&self) -> Option<Paths> {
        self.config_file.as_deref().map(Paths::beside)
    }

    fn selected_package(&self) -> Option<&Installed> {
        let selected = self.selected.as_ref()?;
        self.packages
            .as_ref()?
            .iter()
            .find(|package| &package.file == selected)
    }

    pub(crate) fn content(&self) -> Content {
        let Some(packages) = &self.packages else {
            return Content::Loading;
        };
        if packages.is_empty() {
            return Content::Empty;
        }
        match self.selected_package().map(|package| &package.state) {
            None => Content::Empty,
            Some(Err(reason)) => Content::Refused(reason.clone()),
            Some(Ok(verified)) => match (&self.running, &self.launch_error) {
                (Some(running), _) => match running.failure.borrow().clone() {
                    Some(failure) => Content::Failed(failure),
                    None => Content::Running,
                },
                (None, Some(reason)) => Content::LaunchFailed(reason.clone()),
                (None, None) => Content::NeedsGrant(verified.identity.declared.clone()),
            },
        }
    }

    /// Reads Settings and the installed packages again, off the UI thread.
    fn refresh(&mut self, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(config), Some(paths)) = (self.config_file.clone(), self.paths()) else {
            self.packages = Some(Vec::new());
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        self.latest.store(generation, Ordering::SeqCst);
        let latest = self.latest.clone();
        let task = cx.background_spawn(async move {
            let saved = saved::load(&config);
            let packages = store::installed(&paths, &saved.roots);
            store::locked(|| {
                if latest.load(Ordering::SeqCst) == generation {
                    fields::write_inventory(&paths, &packages);
                }
            });
            (saved, packages)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (saved, packages) = task.await;
            this.update_in(cx, |this, window, cx| {
                // A later refresh read newer Settings: this result is stale.
                if this.generation == generation {
                    this.show(saved, packages, force, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn show(
        &mut self,
        saved: Saved,
        packages: Vec<Installed>,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.saved = saved;
        let keep = self
            .selected
            .clone()
            .filter(|file| packages.iter().any(|package| &package.file == file));
        self.selected = keep.or_else(|| packages.first().map(|package| package.file.clone()));
        self.packages = Some(packages);
        if !force && self.runs_selection() {
            self.show_running(cx);
        } else {
            self.restart(window, cx);
        }
    }

    /// Whether what runs is the selected package as verified now, with the
    /// grants Settings holds now.
    fn runs_selection(&self) -> bool {
        let (Some(running), Some(Ok(verified))) = (
            &self.running,
            self.selected_package().map(|package| &package.state),
        ) else {
            return false;
        };
        running_matches(
            (&running.sha256, &running.granted),
            (&verified.sha256, &verified.identity.granted(&self.saved)),
        )
    }

    /// Settings and packages are as they were: the running package carries
    /// on, sending readings again if the Space is visible.
    fn show_running(&mut self, cx: &mut Context<Self>) {
        let visible = self.visible;
        if let Some(running) = &mut self.running {
            running.set_visible(visible, cx);
        }
        cx.notify();
    }

    /// Stops what runs and starts the selected package with its grants.
    fn restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(running) = self.running.take() {
            running.stop(cx);
        }
        self.launch_error = None;
        let (Some(paths), Some(Ok(verified))) = (
            self.paths(),
            self.selected_package().map(|package| package.state.clone()),
        ) else {
            cx.notify();
            return;
        };
        let granted = verified.identity.granted(&self.saved);
        let waits_for_grant = !verified.identity.declared.is_empty() && granted.is_empty();
        if !waits_for_grant {
            self.launch(&verified, granted, &paths, window, cx);
        }
        cx.notify();
    }

    fn launch(
        &mut self,
        verified: &store::Verified,
        granted: Vec<subspace_tsx_system::Tier>,
        paths: &Paths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match run::start(verified, granted, paths, window, cx) {
            Ok(mut running) => {
                running.set_visible(self.visible, cx);
                self.running = Some(running);
                self.accept(verified.clone(), paths.clone(), cx);
            }
            Err(reason) => self.launch_error = Some(reason),
        }
    }

    /// Records the build once it runs, so older builds are refused, and says
    /// so if that fails.
    fn accept(&self, verified: store::Verified, paths: Paths, cx: &mut Context<Self>) {
        let roots = self.saved.roots.clone();
        let task = cx.background_spawn(async move { store::accept(&paths, &roots, &verified) });
        cx.spawn(async move |this, cx| {
            if let Err(reason) = task.await {
                this.update(cx, |this, cx| {
                    this.notice = Some(reason);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn select(&mut self, file: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.as_ref() != Some(&file) {
            self.selected = Some(file);
            self.restart(window, cx);
        }
    }

    fn toggle_navigator(&mut self, cx: &mut Context<Self>) {
        self.navigator = !self.navigator;
        prefs::set(
            cx,
            NAVIGATOR_KEY,
            if self.navigator { "shown" } else { "hidden" },
        );
        cx.notify();
    }
}

impl TsxSpaces {
    /// Nothing read yet: the config file and the saved navigator choice.
    fn empty(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            config_file: services(cx)
                .map(|services| services.config_file.clone())
                .or_else(subspace_framework::core::config::path),
            saved: Saved::default(),
            packages: None,
            selected: None,
            running: None,
            notice: None,
            launch_error: None,
            generation: 0,
            latest: Arc::default(),
            navigator: prefs::get(cx, NAVIGATOR_KEY).as_deref() != Some("hidden"),
            visible: false,
        }
    }
}

impl EventEmitter<DashboardEvent> for TsxSpaces {}

impl Focusable for TsxSpaces {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TsxSpaces {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        view::render(self, window, cx)
    }
}

impl Dashboard for TsxSpaces {
    const ID: &'static str = "tsx";
    const TITLE: &'static str = "TSX Spaces";
    const DESCRIPTION: &'static str = "Native Spaces from signed TSX packages (experimental)";
    const ICON: Option<&'static str> = Some("icons/tsx.svg");
    const ARTWORK: BoardArtwork =
        BoardArtwork::new(&[("icons/tsx.svg", include_bytes!("../assets/tsx.svg"))]);
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");

    fn settings() -> BoardSettings {
        BoardSettings {
            description: "Runs signed TSX packages built with [nsh](https://nsh.dev) as native Spaces. A package installs only if one of the roots below signed its key, and reads this computer only through the capabilities you allow. Experimental.",
            ..Default::default()
        }
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        host::share_facts(window, cx);
        run::follow_theme(cx);
        let mut this = Self::empty(cx);
        this.refresh(true, window, cx);
        this
    }

    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        // Settings may have changed the roots or the grants.
        self.refresh(false, window, cx);
    }

    fn covered(&mut self, covered: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Settings opens over the Space; when it closes, apply what it saved,
        // such as new or revoked grants, without a Reload, shown or not.
        if reads_settings_again(covered) {
            self.refresh(false, window, cx);
        }
    }

    fn deactivate(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.visible = false;
        if let Some(running) = &mut self.running {
            running.set_visible(false, cx);
        }
    }

    fn status(&self, _cx: &App) -> Option<SharedString> {
        view::status_line(self).map(SharedString::from)
    }

    fn docks(&self, _cx: &App) -> SpaceDocks {
        SpaceDocks {
            navigator: Some(self.navigator),
            inspector: None,
        }
    }

    fn toggle_dock(&mut self, dock: SpaceDock, _window: &mut Window, cx: &mut Context<Self>) {
        if dock == SpaceDock::Navigator {
            self.toggle_navigator(cx);
        }
    }

    fn busy(&self, _cx: &App) -> bool {
        self.packages.is_none()
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.notice = None;
        self.refresh(true, window, cx);
    }
}

/// Whether the Space reads Settings again: when what covered the window,
/// such as Settings, closes.
fn reads_settings_again(covered: bool) -> bool {
    !covered
}

/// Whether a running package, by its hash and grants, is the one to run.
fn running_matches(running: (&String, &Vec<Tier>), current: (&String, &Vec<Tier>)) -> bool {
    running == current
}

#[cfg(test)]
mod tests {
    use super::{reads_settings_again, running_matches};
    use subspace_tsx_system::Tier;

    #[test]
    fn closing_settings_applies_what_it_saved() {
        assert!(reads_settings_again(false));
        assert!(!reads_settings_again(true));
    }

    #[test]
    fn a_revoked_grant_or_a_changed_package_restarts_the_package() {
        let (hash, other) = ("a".to_owned(), "b".to_owned());
        let all = vec![Tier::Metrics, Tier::Apps];
        let fewer = vec![Tier::Metrics];
        assert!(running_matches((&hash, &all), (&hash, &all)));
        assert!(!running_matches((&hash, &all), (&hash, &fewer)));
        assert!(!running_matches((&hash, &all), (&other, &all)));
    }
}
