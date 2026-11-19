//! Favourite repositories: the config file's `repos`, the one list that the
//! GitHub, Copilot Agents and Actions boards and the agent tools share.
//!
//! [`RepositoryPicker`] is the shared search for them: a kept-open
//! [`Menu`] that searches the owner's GitHub account as the user types and
//! stars or unstars a repository on each pick. The owner decides when a
//! change applies: a board writes it at once ([`write`], then [`apply`]),
//! Settings keeps it in its draft until Save.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, SharedString, Subscription, Task, Window,
};

use crate::anchored::Anchor;
use crate::menu::{Menu, MenuAnchors, MenuContents, MenuEvent, MenuOption};
use crate::services;

/// Searches the owner's GitHub account for repositories matching the text
/// (empty: the account's own), returning `owner/name`s. It blocks, so the
/// picker runs it in the background.
pub type RepositorySearch = Arc<dyn Fn(&str) -> Result<Vec<String>, String> + Send + Sync>;

/// How long typing pauses before the picker searches.
const SEARCH_DELAY: Duration = Duration::from_millis(250);
const REPO_PREFIX: &str = "repo:";
const FILTER_VALUE: &str = "filter:favourites-only";
pub const FILTER_SECTION: &str = "Mine, Review requested, Involved";
pub const FILTER_LABEL: &str = "Favourites only";

/// The favourites the app runs with now.
pub fn favourites(cx: &App) -> Vec<String> {
    services(cx)
        .map(|services| services.config.repos.clone())
        .unwrap_or_default()
}

pub fn is_favourite(favourites: &[String], repo: &str) -> bool {
    favourites
        .iter()
        .any(|known| known.eq_ignore_ascii_case(repo))
}

/// `favourites` with `repo` removed when it is one, else added at the end.
pub fn toggled(favourites: &[String], repo: &str) -> Vec<String> {
    let mut next: Vec<String> = favourites
        .iter()
        .filter(|known| !known.eq_ignore_ascii_case(repo))
        .cloned()
        .collect();
    if next.len() == favourites.len() {
        next.push(repo.to_owned());
    }
    next
}

/// Saves `repos` as the favourites through the private config-file editor,
/// returning the list as saved. Blocking file work: run it in the background.
pub fn write(file: &Path, repos: &[String]) -> Result<Vec<String>, String> {
    let mut settings =
        subspace_core::settings::SettingsFile::open(file).map_err(|error| error.to_string())?;
    settings
        .update_repos(repos)
        .map_err(|error| error.to_string())?;
    Ok(settings.values().repos.clone())
}

/// Runs the app with `repos` as its favourites from now on. Boards that
/// observe [`crate::Services`] follow; the shell rebuilds the agent tools on
/// [`crate::DashboardEvent::ConfigChanged`].
pub fn apply(repos: Vec<String>, cx: &mut App) {
    let Some(services) = cx.try_global::<crate::Services>() else {
        return;
    };
    if services.config.repos == repos {
        return;
    }
    let mut config = (*services.config).clone();
    config.repos = repos;
    cx.global_mut::<crate::Services>().config = Arc::new(config);
}

/// What the picker shows: the favourites, the search results and, for an
/// owner that has one, the Favourites only filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PickerView {
    pub favourites: Vec<String>,
    /// `None` until the first search answers.
    pub results: Option<Vec<String>>,
    pub query: String,
    pub filter: Option<bool>,
}

/// The options in order: the filter, the favourites that match the query,
/// then the results that are not favourites yet.
pub fn options(view: &PickerView) -> Vec<MenuOption> {
    let query = view.query.trim().to_lowercase();
    let mut options: Vec<MenuOption> = view
        .filter
        .map(|on| MenuOption {
            section: Some(FILTER_SECTION.into()),
            ..MenuOption::new(FILTER_VALUE, FILTER_LABEL).checked(on)
        })
        .into_iter()
        .collect();
    options.extend(
        view.favourites
            .iter()
            .filter(|repo| query.is_empty() || repo.to_lowercase().contains(&query))
            .map(|repo| repo_option(repo, true, "Favourites")),
    );
    let found = match query.is_empty() {
        true => "Your repositories",
        false => "GitHub results",
    };
    options.extend(
        view.results
            .iter()
            .flatten()
            .filter(|repo| !is_favourite(&view.favourites, repo))
            .map(|repo| repo_option(repo, false, found)),
    );
    options
}

fn repo_option(repo: &str, starred: bool, section: &'static str) -> MenuOption {
    MenuOption {
        starred: Some(starred),
        section: Some(section.into()),
        ..MenuOption::new(format!("{REPO_PREFIX}{repo}"), repo.to_owned())
    }
}

/// What a pick in the picker asks of its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryPickerEvent {
    /// Star `repo` when it is not a favourite, else unstar it.
    Toggle(String),
    /// Turn the Favourites only filter on or off.
    ToggleFilter,
    Closed,
}

/// The pick `value` as the owner's request.
fn picked(value: &str) -> Option<RepositoryPickerEvent> {
    if value == FILTER_VALUE {
        return Some(RepositoryPickerEvent::ToggleFilter);
    }
    value
        .strip_prefix(REPO_PREFIX)
        .map(|repo| RepositoryPickerEvent::Toggle(repo.to_owned()))
}

/// How the owner opens a picker.
pub struct PickerSetup {
    pub favourites: Vec<String>,
    pub filter: Option<bool>,
    pub search: RepositorySearch,
}

/// The open repository search. The owner retains the entity, subscribes to
/// [`RepositoryPickerEvent`] and answers each change with
/// [`RepositoryPicker::set_favourites`].
pub struct RepositoryPicker {
    menu: Entity<Menu>,
    view: PickerView,
    search: RepositorySearch,
    /// The search's state under the options: in progress, empty or failed.
    note: Option<SharedString>,
    /// Counts searches, so only the newest answer shows.
    generation: u64,
    _search: Option<Task<()>>,
    _subscription: Subscription,
}

impl EventEmitter<RepositoryPickerEvent> for RepositoryPicker {}

impl RepositoryPicker {
    /// Opens the search at `anchor`, starting with the account's own
    /// repositories. `None` when the anchor's menu was open, which closes it.
    pub fn open(
        anchors: &MenuAnchors,
        anchor: Entity<Anchor>,
        setup: PickerSetup,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Entity<Self>> {
        let view = PickerView {
            favourites: setup.favourites,
            filter: setup.filter,
            ..PickerView::default()
        };
        let contents = MenuContents::new(
            "Repositories",
            "Search repositories on GitHub",
            options(&view),
        )
        .keep_open(true)
        .owner_filtered(true);
        let menu = anchors.open(anchor, contents, window, cx)?;
        let picker = cx.new(|cx| {
            let subscription = cx.subscribe(&menu, |picker: &mut Self, _, event, cx| {
                picker.on_menu(event, cx);
            });
            Self {
                menu,
                view,
                search: setup.search,
                note: None,
                generation: 0,
                _search: None,
                _subscription: subscription,
            }
        });
        picker.update(cx, |picker, cx| picker.run_search(String::new(), false, cx));
        Some(picker)
    }

    /// The favourites and filter after the owner applied a change.
    pub fn set_favourites(
        &mut self,
        favourites: Vec<String>,
        filter: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        self.view.favourites = favourites;
        self.view.filter = filter;
        self.refresh(cx);
    }

    pub fn view(&self) -> &PickerView {
        &self.view
    }

    fn on_menu(&mut self, event: &MenuEvent, cx: &mut Context<Self>) {
        match event {
            MenuEvent::Pick(value) => {
                if let Some(request) = picked(value) {
                    cx.emit(request);
                }
            }
            MenuEvent::Query(text) => {
                self.view.query = text.to_string();
                self.run_search(text.to_string(), true, cx);
            }
            MenuEvent::Close => cx.emit(RepositoryPickerEvent::Closed),
        }
    }

    /// Searches for `text` in the background, after a pause when `delay`.
    fn run_search(&mut self, text: String, delay: bool, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let search = self.search.clone();
        self.note = Some("Searching…".into());
        self.refresh(cx);
        self._search = Some(cx.spawn(async move |this, cx| {
            if delay {
                cx.background_executor().timer(SEARCH_DELAY).await;
            }
            let result = cx.background_spawn(async move { search(&text) }).await;
            _ = this.update(cx, |picker, cx| picker.answered(generation, result, cx));
        }));
    }

    fn answered(
        &mut self,
        generation: u64,
        result: Result<Vec<String>, String>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            return;
        }
        self.note = match result {
            Ok(found) => {
                let none = found.is_empty();
                self.view.results = Some(found);
                none.then(|| "No repositories found".into())
            }
            Err(error) => Some(format!("Search failed: {error}").into()),
        };
        self.refresh(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let options = options(&self.view);
        let note = self.note.clone();
        self.menu.update(cx, |menu, cx| {
            menu.set_options(options, cx);
            menu.set_note(note, cx);
        });
    }
}

#[cfg(test)]
mod tests;
