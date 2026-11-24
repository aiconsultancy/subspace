//! The Space's own changes: the reads it starts and the answers it applies,
//! the navigator's and toolbar's choices, and the list lines drawn from the
//! loaded decisions.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use gpui_kit::{App, AppContext as _, Context, SharedString, Window};
use subspace_api_client::Error;
use subspace_api_client::model::DecisionListItem;
use subspace_framework::dashboard::{ListState, Row, Tokens, list, prefs, workspace};

use crate::load::{self, Answer, Directory, Failure, Found, Leased, Plan, Stop};
use crate::model::{self, Choice, Maker, ProjectChoice, View};
use crate::state::{self, Listing, Session, Status};
use crate::{Decisions, listing, search, words};

const VIEW_PREF: &str = "decisions.view";
const MAKER_PREF: &str = "decisions.maker";
const PROJECT_PREF: &str = "decisions.project";
/// Showing the Space reads the list again when it is older than this.
const STALE_AFTER: Duration = Duration::from_secs(60);

/// Which groups have had their first fold: Superseded and Rejected start
/// folded in All decisions. A new view or search starts again.
#[derive(Default)]
pub(crate) struct Folds {
    key: Option<(View, bool)>,
    seen: HashSet<SharedString>,
}

pub(crate) fn new(window: &mut Window, cx: &mut Context<Decisions>) -> Decisions {
    let (search, typing) = search::input(window, cx);
    Decisions {
        weak: cx.entity().downgrade(),
        focus: cx.focus_handle(),
        list: ListState::default(),
        choice: saved_choice(cx),
        search,
        query: String::new(),
        docks: crate::docks::Docks::load(cx),
        session: Session::default(),
        listing: Listing::default(),
        page: None,
        author: None,
        history: Default::default(),
        #[cfg(test)]
        test_writer: None,
        showing_page: false,
        reader: load::Reader::service(),
        reads: 0,
        stop: Stop::default(),
        page_reads: 0,
        lease: None,
        account_epoch: 0,
        active: false,
        tidy: false,
        folds: Folds::default(),
        text_edge: workspace::text_edge(&Tokens::current(cx)),
        menu_anchors: Default::default(),
        menu: None,
        _read: None,
        _page_read: None,
        _search_task: None,
        _watch: crate::account::watch(window, cx),
        _subscriptions: vec![typing],
    }
}

/// The view, maker and project chosen last time. The tag starts clear.
fn saved_choice(cx: &App) -> Choice {
    Choice {
        view: prefs::get(cx, VIEW_PREF)
            .and_then(|key| View::from_key(&key))
            .unwrap_or_default(),
        maker: prefs::get(cx, MAKER_PREF)
            .and_then(|key| Maker::from_key(&key))
            .unwrap_or_default(),
        project: prefs::get(cx, PROJECT_PREF)
            .map(|key| ProjectChoice::from_key(&key))
            .unwrap_or_default(),
        tag: None,
    }
}

// qual:allow(coupling, oi) reason: "The Space's reads and choices change the state the render reads; the rules they follow are pure and tested in model, listing and state."
impl Decisions {
    /// Reads the list for the current choice and search. `directory` reads
    /// the account, its projects and its people first, as a read does
    /// anyway while no account is known.
    pub(crate) fn read(&mut self, directory: bool, cx: &mut Context<Self>) {
        self.reads = self.reads.wrapping_add(1);
        let number = self.reads;
        // The read this one supersedes stops before its next page.
        std::mem::take(&mut self.stop).stop();
        let plan = Plan {
            query: model::query(&self.choice, &self.query),
            directory: directory || self.session.account.is_none(),
            stop: self.stop.clone(),
        };
        let reader = self.reader.list.clone();
        self.listing.status = Status::Loading;
        let read = cx.background_spawn(async move { reader(&plan) });
        self._read = Some(cx.spawn(async move |this, cx| {
            let answer = read.await;
            _ = this.update(cx, |this, cx| this.finish_read(number, answer, cx));
        }));
        self.sync_lines(cx);
    }

    fn finish_read(&mut self, number: u64, answer: Leased<Answer>, cx: &mut Context<Self>) {
        let (lease, answer) = answer.split(|error| Answer {
            directory: None,
            found: Err(error),
        });
        if self.landed(number == self.reads, lease.as_ref(), cx) {
            self.apply(answer, Instant::now(), cx);
        }
    }

    /// Showing the Space reads the list again when it is stale.
    pub(crate) fn read_if_stale(&mut self, cx: &mut Context<Self>) {
        let fresh = self
            .listing
            .read_at
            .is_some_and(|read_at| read_at.elapsed() < STALE_AFTER);
        if !fresh && !self.listing.loading() {
            self.read(true, cx);
        }
    }

    /// The Space follows one answer: the account first, then the list.
    pub(crate) fn apply(&mut self, answer: Answer, now: Instant, cx: &mut Context<Self>) {
        if let Some(directory) = answer.directory {
            self.apply_directory(directory);
        }
        match answer.found {
            Ok(found) => self.apply_found(found, now),
            Err(error) => self.apply_failure(load::failure(error)),
        }
        self.sync_lines(cx);
    }

    /// A failed directory read fails the list read too, which decides what
    /// stays (`apply_failure`).
    fn apply_directory(&mut self, directory: Result<Directory, Error>) {
        let Ok(directory) = directory else {
            return;
        };
        self.session.account = Some(directory.me);
        self.session.projects = directory.projects;
        self.session.people = directory.people;
        self.session.human_makers = directory.human_makers;
    }

    fn apply_found(&mut self, found: Found, now: Instant) {
        let tags = found
            .items
            .iter()
            .flat_map(|item| item.tags.iter().cloned());
        self.session.tags.extend(tags);
        self.listing = Listing {
            items: Some(found.items),
            more: found.more,
            read_at: Some(now),
            status: Status::Idle,
        };
    }

    /// An account-level failure keeps nothing read with that account.
    fn apply_failure(&mut self, failure: Failure) {
        if failure.account_level() {
            self.session = Session::default();
            self.listing.items = None;
            self.page = None;
            self.showing_page = false;
        }
        self.listing.status = Status::Failed(failure);
    }

    /// Draws the loaded decisions as the list's groups, keeping the
    /// selection while its row is still shown.
    pub(crate) fn sync_lines(&mut self, cx: &mut Context<Self>) {
        let groups = self
            .listing
            .items
            .as_deref()
            .map(listing::groups)
            .unwrap_or_default();
        self.list.set_groups(groups);
        self.fold_new_groups();
        self.list.set_status(state::list_status(&self.listing));
        cx.notify();
    }

    /// Gives each group its first fold once per view and search.
    fn fold_new_groups(&mut self) {
        let key = (self.choice.view, !self.query.is_empty());
        if self.folds.key != Some(key) {
            self.folds.key = Some(key);
            self.folds.seen.clear();
        }
        let groups: Vec<SharedString> = self.list.groups().iter().map(|g| g.key.clone()).collect();
        for group in groups {
            if !self.folds.seen.insert(group.clone()) {
                continue;
            }
            if self.list.is_folded(&group) != listing::starts_folded(key.0, key.1, &group) {
                self.list.toggle(&group);
            }
        }
    }

    pub(crate) fn choose_view(&mut self, view: View, cx: &mut Context<Self>) {
        if self.choice.view != view {
            self.choice.view = view;
            prefs::set(cx, VIEW_PREF, view.key());
            self.choice_changed(cx);
        }
        self.show_list(cx);
    }

    pub(crate) fn choose_maker(&mut self, maker: Maker, cx: &mut Context<Self>) {
        if self.choice.maker != maker {
            self.choice.maker = maker;
            prefs::set(cx, MAKER_PREF, maker.key());
            self.choice_changed(cx);
        }
        self.show_list(cx);
    }

    pub(crate) fn choose_project(&mut self, project: ProjectChoice, cx: &mut Context<Self>) {
        if self.choice.project != project {
            prefs::set(cx, PROJECT_PREF, project.key());
            self.choice.project = project;
            self.choice_changed(cx);
        }
        self.show_list(cx);
    }

    /// `None` shows every tag. The tag is not kept between runs.
    pub(crate) fn choose_tag(&mut self, tag: Option<String>, cx: &mut Context<Self>) {
        if self.choice.tag != tag {
            self.choice.tag = tag;
            self.choice_changed(cx);
        }
        self.show_list(cx);
    }

    /// A new choice starts from nothing, so no other list shows under it.
    fn choice_changed(&mut self, cx: &mut Context<Self>) {
        self.listing.items = None;
        self.read(false, cx);
    }

    /// The selected decision, when a decision row is selected.
    pub(crate) fn selected_item(&self) -> Option<&DecisionListItem> {
        self.item(listing::decision_id(self.list.selected()?)?)
    }

    /// Decision `id`, when the list holds it.
    pub(crate) fn item(&self, id: &str) -> Option<&DecisionListItem> {
        self.listing
            .items
            .as_ref()?
            .iter()
            .find(|item| item.id == id)
    }

    /// Opens decision `id` as a page, named by its row until its read lands.
    pub(crate) fn open_decision(&mut self, id: &str, cx: &mut Context<Self>) {
        let title = self
            .item(id)
            .map_or_else(|| "Decision".to_owned(), |item| item.title.clone());
        self.open_page(id.to_owned(), title, cx);
    }

    /// Enter, a double click and the inspector's Open.
    pub(crate) fn open_selected(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_item().map(|item| item.id.clone()) else {
            return;
        };
        self.open_decision(&id, cx);
    }

    /// A supersession link: on the page it opens that decision; in the list
    /// it selects that decision's row, unfolding its group, or opens it
    /// when the list does not hold it.
    pub(crate) fn follow(&mut self, id: &str, title: &str, cx: &mut Context<Self>) {
        if self.follow_account(cx) {
            return;
        }
        let group = self
            .item(id)
            .filter(|_| !self.showing_page)
            .map(|item| listing::group_key(item.status));
        let Some(group) = group else {
            self.open_page(id.to_owned(), title.to_owned(), cx);
            return;
        };
        let key = listing::row_key(id);
        list::update(self, cx, move |list| {
            if list.is_folded(&group) {
                list.toggle(&group);
            }
            list.select(key);
        });
    }

    /// A row's spoken name.
    pub(crate) fn row_label(&self, row: &Row) -> SharedString {
        listing::decision_id(&row.key)
            .and_then(|id| self.item(id))
            .map_or_else(
                || row.title.clone(),
                |item| words::row_label(item, &self.session.people).into(),
            )
    }

    pub(crate) fn status_line(&self, now: Instant) -> String {
        let service = subspace_api_client::service_name();
        if self.account_moved() {
            return service.to_owned();
        }
        state::status_line(service, self.session.account_name(), &self.listing, now)
    }

    pub(crate) fn account_line(&self) -> String {
        state::account_line(
            subspace_api_client::service_name(),
            self.session.account_name(),
            &self.listing,
        )
    }
}
