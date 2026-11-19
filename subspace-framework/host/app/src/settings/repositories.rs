//! The favourite repositories in Settings › PRs (and the other GitHub
//! boards' pages): a draft list with a Remove button per repository and
//! **Add repository…**, which opens the shared repository search. Unlike the
//! PRs board's own Repositories control, which saves each star at once,
//! these edits stay in the draft until Save.

use gpui_kit::{App, Context, Entity, Subscription, Window};
use subspace_dashboard::anchored::Anchor;
use subspace_dashboard::menu::MenuAnchors;
use subspace_dashboard::repositories::{
    PickerSetup, RepositoryPicker, RepositoryPickerEvent, RepositorySearch, toggled,
};

/// The anchor of the Add repository… button.
pub(super) const ADD_ANCHOR: &str = "settings-repositories-add";

pub(super) struct RepositoryList {
    favourites: Vec<String>,
    anchors: MenuAnchors,
    picker: Option<(Entity<RepositoryPicker>, Subscription)>,
}

impl RepositoryList {
    pub(super) fn new(favourites: Vec<String>) -> Self {
        Self {
            favourites,
            anchors: MenuAnchors::default(),
            picker: None,
        }
    }

    pub(super) fn favourites(&self) -> &[String] {
        &self.favourites
    }

    pub(super) fn anchors(&self) -> &MenuAnchors {
        &self.anchors
    }

    pub(super) fn anchor(&self, cx: &mut App) -> Entity<Anchor> {
        self.anchors.anchor(ADD_ANCHOR, cx)
    }

    /// Stars `repo` when it is not in the draft, else removes it.
    pub(super) fn toggle(&mut self, repo: &str, cx: &mut Context<Self>) {
        self.favourites = toggled(&self.favourites, repo);
        let favourites = self.favourites.clone();
        if let Some((picker, _)) = &self.picker {
            picker.update(cx, |picker, cx| picker.set_favourites(favourites, None, cx));
        }
        cx.notify();
    }

    /// Opens the repository search under Add repository….
    pub(super) fn open_search(
        &mut self,
        search: RepositorySearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let setup = PickerSetup {
            favourites: self.favourites.clone(),
            filter: None,
            search,
        };
        let anchor = self.anchor(cx);
        let Some(picker) = RepositoryPicker::open(&self.anchors, anchor, setup, window, cx) else {
            self.picker = None;
            return;
        };
        let subscription = cx.subscribe(&picker, |list, _, event, cx| match event {
            RepositoryPickerEvent::Toggle(repo) => list.toggle(repo, cx),
            RepositoryPickerEvent::ToggleFilter => {}
            RepositoryPickerEvent::Closed => {
                list.picker = None;
                cx.notify();
            }
        });
        self.picker = Some((picker, subscription));
        cx.notify();
    }
}

/// The GitHub repository search as `account` (the Settings account, else
/// the active gh account), in the background.
pub(super) fn github_search(account: Option<String>, cx: &App) -> Option<RepositorySearch> {
    let services = cx.try_global::<subspace_dashboard::Services>()?;
    let client = subspace_client_github::Client::new(services.credentials.clone());
    Some(std::sync::Arc::new(move |text: &str| {
        let login = match &account {
            Some(login) => login.clone(),
            None => client
                .accounts()
                .ok()
                .and_then(|accounts| accounts.active.or_else(|| accounts.logins.first().cloned()))
                .ok_or("No GitHub account is signed in.")?,
        };
        client
            .repositories(&login, text)
            .map(|found| found.into_iter().map(|repo| repo.full_name).collect())
            .map_err(|error| error.to_string())
    }))
}

impl super::Settings {
    /// Add repository…: the search as the account the draft selects.
    pub(super) fn add_repository(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let account = self.account.read(cx).selected_value().cloned();
        let Some(search) = github_search(account, cx) else {
            return;
        };
        self.form
            .repos
            .update(cx, |list, cx| list.open_search(search, window, cx));
    }

    pub(super) fn remove_repository(&mut self, repo: &str, cx: &mut Context<Self>) {
        self.form.repos.update(cx, |list, cx| list.toggle(repo, cx));
        cx.notify();
    }
}
