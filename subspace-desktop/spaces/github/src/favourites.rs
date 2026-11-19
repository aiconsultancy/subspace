//! Favourite repositories in the PRs board: the Repositories control (the
//! shared repository search with stars), the star on repository headers and
//! the Favourites only filter.
//!
//! The favourites are the config file's `repos`. A star saves at once
//! through the private config-file editor, then the app runs with the new
//! list ([`repositories::apply`]) and the shell rebuilds the agent tools
//! ([`DashboardEvent::ConfigChanged`]). The board follows `Services`, so a
//! Settings Save reaches it the same way.

use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, Entity, Subscription, Window};
use subspace_framework::client_github::Client;
use subspace_framework::dashboard::anchored::Anchor;
use subspace_framework::dashboard::repositories::{
    self, PickerSetup, RepositoryPicker, RepositoryPickerEvent, RepositorySearch, is_favourite,
    toggled,
};
use subspace_framework::dashboard::{DashboardEvent, Services, Toast, prefs, services};

use crate::Github;
use crate::model::{Scope, section_by_id};

/// Whether Favourites only is on, `1` or `0`.
pub(crate) const ONLY_KEY: &str = "github.favourites-only";
/// The Repositories control's anchor and toolbar unit.
pub(crate) const ANCHOR: &str = "gh-repos";

/// The board's favourites and the open search.
pub(crate) struct Favourites {
    /// What the board shows and searches: the saved list, or the list being
    /// saved.
    pub(crate) list: Vec<String>,
    /// The running config's list that the board last followed.
    applied: Vec<String>,
    pub(crate) only: bool,
    picker: Option<(Entity<RepositoryPicker>, Subscription)>,
    saving: bool,
    /// The newest list to save after the one in flight.
    pending: Option<Vec<String>>,
    _config: Subscription,
}

impl Favourites {
    pub(crate) fn new(cx: &mut Context<Github>) -> Self {
        let list = repositories::favourites(cx);
        Self {
            applied: list.clone(),
            list,
            only: prefs::get(cx, ONLY_KEY).as_deref() == Some("1"),
            picker: None,
            saving: false,
            pending: None,
            _config: cx.observe_global::<Services>(Github::follow_config),
        }
    }

    pub(crate) fn scope(&self) -> Scope {
        Scope {
            has_favourites: !self.list.is_empty(),
            favourites_only: self.only,
        }
    }

    pub(crate) fn contains(&self, repo: &str) -> bool {
        is_favourite(&self.list, repo)
    }

    pub(crate) fn is_open(&self) -> bool {
        self.picker.is_some()
    }

    pub(crate) fn close(&mut self) {
        self.picker = None;
    }
}

/// The repository search as `login`, returning `owner/name`s.
fn search(client: Arc<Client>, login: String) -> RepositorySearch {
    Arc::new(move |text: &str| {
        client
            .repositories(&login, text)
            .map(|found| found.into_iter().map(|repo| repo.full_name).collect())
            .map_err(|error| error.to_string())
    })
}

/// The section ids whose search depends on the favourites.
pub(crate) fn scoped_sections(only: bool) -> Vec<&'static str> {
    ["mine", "review", "involved", "open"]
        .into_iter()
        .filter(|id| only || section_by_id(id).is_some_and(|section| section.repos_only))
        .collect()
}

impl Github {
    /// Opens the Repositories search at `anchor`, else at the toolbar control.
    pub(crate) fn open_repositories(
        &mut self,
        anchor: Option<Entity<Anchor>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(client), Some(login)) = (self.client.clone(), self.account.clone()) else {
            cx.emit(DashboardEvent::Toast(Toast::error(
                "Sign in to GitHub to search repositories.",
            )));
            return;
        };
        let anchor = anchor.unwrap_or_else(|| self.menu_anchors.anchor(ANCHOR, cx));
        let setup = PickerSetup {
            favourites: self.favourites.list.clone(),
            filter: Some(self.favourites.only),
            search: search(client, login),
        };
        self.menu = None;
        let picker = RepositoryPicker::open(&self.menu_anchors, anchor, setup, window, cx);
        self.favourites.picker = picker.map(|picker| {
            let subscription = cx.subscribe(&picker, |this, _, event, cx| match event {
                RepositoryPickerEvent::Toggle(repo) => this.toggle_favourite(repo.clone(), cx),
                RepositoryPickerEvent::ToggleFilter => this.toggle_favourites_only(cx),
                RepositoryPickerEvent::Closed => {
                    this.favourites.close();
                    cx.notify();
                }
            });
            (picker, subscription)
        });
        cx.notify();
    }

    /// Stars `repo`, or unstars it, and saves the list at once.
    pub(crate) fn toggle_favourite(&mut self, repo: String, cx: &mut Context<Self>) {
        let next = toggled(&self.favourites.list, &repo);
        self.favourites.list.clone_from(&next);
        self.refresh_picker(cx);
        self.rebuild(cx);
        match self.favourites.saving {
            true => self.favourites.pending = Some(next),
            false => self.save_favourites(next, cx),
        }
    }

    fn save_favourites(&mut self, list: Vec<String>, cx: &mut Context<Self>) {
        let Some(file) = services(cx).map(|services| services.config_file.clone()) else {
            return;
        };
        self.favourites.saving = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { repositories::write(&file, &list) })
                .await;
            _ = this.update(cx, |this, cx| this.saved_favourites(result, cx));
        })
        .detach();
    }

    fn saved_favourites(&mut self, result: Result<Vec<String>, String>, cx: &mut Context<Self>) {
        match result {
            Ok(saved) => {
                match self.favourites.pending.take() {
                    Some(next) => self.save_favourites(next, cx),
                    None => self.favourites.saving = false,
                }
                repositories::apply(saved, cx);
                cx.emit(DashboardEvent::ConfigChanged);
            }
            Err(error) => {
                self.favourites.saving = false;
                self.favourites.pending = None;
                self.favourites.list = repositories::favourites(cx);
                self.refresh_picker(cx);
                self.rebuild(cx);
                let message = format!("Favourites not saved: {error}");
                cx.emit(DashboardEvent::Toast(Toast::error(message)));
            }
        }
    }

    /// Favourites only on or off, saved as a preference; the personal
    /// sections reload with their new scope.
    pub(crate) fn toggle_favourites_only(&mut self, cx: &mut Context<Self>) {
        self.favourites.only = !self.favourites.only;
        let value = if self.favourites.only { "1" } else { "0" };
        prefs::set(cx, ONLY_KEY, value);
        self.refresh_picker(cx);
        self.reload_sections(&["mine", "review", "involved"], cx);
    }

    /// Follows the running config: a saved star here, or a Settings Save.
    fn follow_config(&mut self, cx: &mut Context<Self>) {
        let current = repositories::favourites(cx);
        if current == self.favourites.applied {
            return;
        }
        self.favourites.applied.clone_from(&current);
        if !self.favourites.saving {
            self.favourites.list = current;
        }
        self.refresh_picker(cx);
        let sections = scoped_sections(self.favourites.only);
        self.reload_sections(&sections, cx);
    }

    /// Loads `sections` again for the account, with the current scope.
    fn reload_sections(&mut self, sections: &[&'static str], cx: &mut Context<Self>) {
        if let (Some(client), Some(login)) = (self.client.clone(), self.account.clone()) {
            for section in sections {
                self.load_section(client.clone(), login.clone(), section, cx);
            }
        }
        self.rebuild(cx);
    }

    fn refresh_picker(&self, cx: &mut App) {
        if let Some((picker, _)) = &self.favourites.picker {
            let (list, only) = (self.favourites.list.clone(), Some(self.favourites.only));
            picker.update(cx, |picker, cx| picker.set_favourites(list, only, cx));
        }
    }

    /// The repository of the selected header, or of the selected row.
    pub(crate) fn selected_repo(&self) -> Option<String> {
        let key = self.list.selected()?;
        match key.strip_prefix("repo:") {
            Some(repo) => Some(repo.to_owned()),
            None => self.find_item(key).map(|item| item.repo.clone()),
        }
    }

    /// `f`: star or unstar the selected repository.
    pub(crate) fn toggle_selected_favourite(&mut self, cx: &mut Context<Self>) {
        if self.diff.is_some() {
            return;
        }
        match self.selected_repo() {
            Some(repo) => self.toggle_favourite(repo, cx),
            None => cx.emit(DashboardEvent::Toast(Toast::error(
                "Select a repository or a pull request first",
            ))),
        }
    }
}

/// The verb for starring or unstarring `repo`.
pub(crate) fn toggle_label(repo: &str, favourite: bool) -> String {
    match favourite {
        true => format!("Remove {repo} from favourites"),
        false => format!("Add {repo} to favourites"),
    }
}

#[cfg(test)]
mod tests {
    use super::{scoped_sections, toggle_label};

    #[test]
    fn favourites_rescope_all_open_and_with_the_filter_the_personal_sections() {
        assert_eq!(scoped_sections(false), ["open"]);
        assert_eq!(
            scoped_sections(true),
            ["mine", "review", "involved", "open"]
        );
    }

    #[test]
    fn the_star_command_names_its_repository_and_result() {
        assert_eq!(
            toggle_label("acme/web", false),
            "Add acme/web to favourites"
        );
        assert_eq!(
            toggle_label("acme/web", true),
            "Remove acme/web from favourites"
        );
    }
}
