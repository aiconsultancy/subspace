//! The two sources produce the same files for the viewer. Fleet snapshots
//! are never cached outside the open view or exposed as GitHub subjects.

use std::collections::HashSet;
use std::sync::Arc;

use gpui_kit::Subscription;
use subspace_client_fleet::diff::{DiffMode, DiffRead, DiffStatus, DiffTarget, V1Diff, V1DiffFile};
use subspace_client_fleet::navigation::OpenCrewDiff;
use subspace_client_fleet::{Error, FleetApi, FleetClient, FleetFollow};
use subspace_framework::client_github::Client;
use subspace_framework::client_github::model::{DiffResponse, GhDiffFile, GhFileStatus, GhItem};

use super::FileView;

#[derive(Clone)]
pub(crate) enum DiffSource {
    GitHub(GitHubSource),
    Fleet(FleetSource),
}

#[derive(Clone)]
pub(crate) struct GitHubSource {
    pub item: Box<GhItem>,
    pub login: String,
    pub client: Option<Arc<Client>>,
}

#[derive(Clone)]
pub(crate) struct FleetSource {
    pub target: OpenCrewDiff,
}

#[derive(Default)]
pub(super) struct FleetState {
    pub mode: DiffMode,
    pub fingerprint: Option<String>,
    pub live_fingerprint: Option<String>,
    pub live_revision: u64,
    pub live_changed: bool,
    pub unstable: bool,
    pub busy: Option<String>,
    pub unavailable: bool,
    pub stale_page: bool,
    pub loading: HashSet<usize>,
    pub generation: u64,
    pub read: DiffRead,
    pub watch: Option<(FleetFollow, Subscription)>,
}

impl FleetState {
    pub fn changed(&self) -> bool {
        self.stale_page || self.live_changed
    }

    /// A repeat of the cached value (including an unrelated service notify)
    /// is not evidence of a newer checkout. Missing values prove nothing.
    pub fn observe_fingerprint(&mut self, live: Option<String>) {
        if let Some(live) = live
            && self.live_fingerprint.as_ref() != Some(&live)
        {
            self.live_changed = self.fingerprint.as_ref().is_some_and(|at| at != &live);
            self.live_fingerprint = Some(live);
            self.live_revision += 1;
        }
    }

    pub fn accept_fingerprint(&mut self, at: String, baseline: u64) {
        self.live_changed = self.live_revision != baseline
            && self
                .live_fingerprint
                .as_ref()
                .is_some_and(|live| live != &at);
        self.fingerprint = Some(at);
    }

    pub fn banner(&self) -> Option<&'static str> {
        if self.unstable {
            Some("Changed while loading")
        } else {
            self.changed().then_some("Changed since loaded")
        }
    }
}

pub(super) enum Loaded {
    GitHub(DiffResponse),
    Fleet(V1Diff),
    Unavailable,
}

impl DiffSource {
    pub fn github(&self) -> Option<&GitHubSource> {
        match self {
            Self::GitHub(source) => Some(source),
            Self::Fleet(_) => None,
        }
    }
}

impl FleetSource {
    pub fn request(&self, mode: DiffMode) -> DiffTarget<'_> {
        DiffTarget {
            ship: &self.target.ship,
            member: &self.target.member,
            project: self.target.project.as_deref(),
            mode,
        }
    }

    pub fn client(&self, cx: &gpui_kit::App) -> Result<FleetClient, String> {
        FleetApi::current(cx)
            .ok_or_else(|| "Fleet connections are unavailable.".to_owned())?
            .read(cx)
            .client(&self.target.fleet)
            .map_err(|state| state.help().unwrap_or_else(|| state.label()).to_owned())
    }

    pub(super) fn load(
        &self,
        client: &FleetClient,
        mode: DiffMode,
        read: &DiffRead,
    ) -> Result<Loaded, Error> {
        let request = self.request(mode);
        load_pages(read, |cursor| read.retry(|| client.diff(request, cursor)))
    }
}

fn load_pages(
    cancel: &DiffRead,
    mut read: impl FnMut(Option<&str>) -> Result<Option<V1Diff>, Error>,
) -> Result<Loaded, Error> {
    let Some(mut diff) = read(None)? else {
        return Ok(Loaded::Unavailable);
    };
    let mut cursors = HashSet::new();
    while let Some(cursor) = diff.next_cursor.take() {
        cancel.check()?;
        if diff.files.len() >= diff.total as usize || !cursors.insert(cursor.clone()) {
            return Err(Error::InvalidResponse);
        }
        let mut page = read(Some(&cursor))?.ok_or(Error::StaleDiff)?;
        if page.fingerprint != diff.fingerprint {
            return Err(Error::StaleDiff);
        }
        if page.files.is_empty() || !same_checkout(&diff, &page) {
            return Err(Error::InvalidResponse);
        }
        diff.files.append(&mut page.files);
        diff.unstable |= page.unstable;
        diff.next_cursor = page.next_cursor;
    }
    if diff.files.len() != diff.total as usize {
        return Err(Error::InvalidResponse);
    }
    Ok(Loaded::Fleet(diff))
}

fn same_checkout(left: &V1Diff, right: &V1Diff) -> bool {
    left.ship == right.ship
        && left.member == right.member
        && left.project == right.project
        && left.mode == right.mode
        && left.total == right.total
}

// qual:allow(dry, boilerplate) reason: "The Fleet and GitHub client enums are independent wire types; the viewer owns their mapping."
pub(super) fn fleet_file(file: V1DiffFile) -> FileView {
    let status = match file.status {
        DiffStatus::Added => GhFileStatus::Added,
        DiffStatus::Removed => GhFileStatus::Removed,
        DiffStatus::Modified => GhFileStatus::Modified,
        DiffStatus::Renamed => GhFileStatus::Renamed,
    };
    let mut view = FileView::new(GhDiffFile {
        path: file.path,
        previous_path: file.previous_path,
        status,
        additions: file.additions.unwrap_or_default(),
        deletions: file.deletions.unwrap_or_default(),
        patch: file.patch,
    });
    view.binary = file.binary;
    if file.binary {
        view.binary_size = file.size;
        view.reason = Some("Binary file");
    } else {
        view.next_page = file.patch_next_page;
        if view.file.patch.is_none() && view.next_page.is_some() {
            view.reason = None;
        }
    }
    view
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use subspace_client_fleet::{Error, diff::V1Diff};
    use subspace_framework::client_github::model::GhFileStatus;

    use super::{DiffRead, FleetState, Loaded, fleet_file};

    fn load_pages(
        read: impl FnMut(Option<&str>) -> Result<Option<V1Diff>, Error>,
    ) -> Result<Loaded, Error> {
        super::load_pages(&DiffRead::default(), read)
    }

    fn page(path: &str, fingerprint: &str, cursor: Option<&str>) -> V1Diff {
        serde_json::from_value(json!({
            "ship":"example-ship", "member":"example-crew", "project":"example", "mode":"branch",
            "branch":"example", "base":null, "head":"head", "fingerprint":fingerprint,
            "computedAt":123, "total":2, "additions":1, "deletions":0, "nextCursor":cursor,
            "files":[{"path":path, "previousPath":"old.txt", "status":"renamed", "additions":1,
                      "deletions":0, "patch":"@@ -0,0 +1 @@\n+example.com\n", "patchNextPage":"more"}]
        })).unwrap()
    }

    #[test]
    fn cuts_a_fleet_snapshot_into_the_same_viewer_models_as_a_pr() {
        let loaded = load_pages(|cursor| {
            Ok(Some(match cursor {
                None => page("a.txt", "snapshot", Some("next")),
                Some("next") => page("b.txt", "snapshot", None),
                Some(other) => panic!("unexpected cursor {other}"),
            }))
        })
        .unwrap();
        let Loaded::Fleet(diff) = loaded else {
            panic!("expected Fleet diff");
        };
        let files: Vec<_> = diff.files.into_iter().map(fleet_file).collect();
        assert_eq!(files[0].file.status, GhFileStatus::Renamed);
        assert_eq!(files[0].file.previous_path.as_deref(), Some("old.txt"));
        assert_eq!(files[0].patch.lines[0].text, "example.com");
        assert_eq!(files[0].next_page.as_deref(), Some("more"));
        assert_eq!(files[1].file.path, "b.txt");
    }

    #[test]
    fn does_not_mix_snapshots_or_follow_a_cyclic_cursor() {
        let changed = load_pages(|cursor| {
            Ok(Some(match cursor {
                None => page("a.txt", "before", Some("next")),
                Some(_) => page("b.txt", "after", None),
            }))
        });
        assert_eq!(changed.err(), Some(Error::StaleDiff));
        let repeats = load_pages(|_| Ok(Some(page("a.txt", "snapshot", Some("next")))));
        assert_eq!(repeats.err(), Some(Error::InvalidResponse));
        assert!(matches!(load_pages(|_| Ok(None)), Ok(Loaded::Unavailable)));
    }

    #[test]
    fn cached_feed_does_not_block_paging_of_a_newer_http_snapshot() {
        let mut state = FleetState::default();
        state.observe_fingerprint(Some("A".into()));
        state.fingerprint = Some("B".into());
        for live in [None, Some("A"), Some("B")] {
            state.observe_fingerprint(live.map(str::to_owned));
            assert!(!state.changed(), "paging must remain enabled");
            assert_eq!(state.banner(), None);
        }
        state.observe_fingerprint(Some("C".into()));
        assert!(state.changed());
        state.observe_fingerprint(Some("B".into()));
        state.stale_page = true;
        assert!(state.changed());
    }

    #[test]
    fn keeps_content_and_marks_an_unstable_snapshot_on_either_list_page() {
        for (initial, continuation) in [(true, false), (false, true)] {
            let loaded = load_pages(|cursor| {
                let mut page = if cursor.is_none() {
                    page("a.txt", "snapshot", continuation.then_some("next"))
                } else {
                    page("b.txt", "snapshot", None)
                };
                if !continuation {
                    page.total = 1;
                }
                page.unstable = if cursor.is_none() {
                    initial
                } else {
                    continuation
                };
                Ok(Some(page))
            })
            .unwrap();
            let Loaded::Fleet(diff) = loaded else {
                panic!("expected Fleet diff")
            };
            assert_eq!(diff.files.len(), if continuation { 2 } else { 1 });
            let state = FleetState {
                unstable: diff.unstable,
                ..FleetState::default()
            };
            assert_eq!(state.banner(), Some("Changed while loading"));
            assert!(
                !state.changed(),
                "instability alone must not disable paging"
            );
        }
    }

    #[test]
    fn an_unborn_empty_snapshot_is_not_unavailable() {
        let mut unborn = page("notes.txt", "snapshot", None);
        unborn.files.clear();
        unborn.head = None;
        unborn.total = 0;
        let mut fixture = Some(unborn);
        assert!(
            matches!(load_pages(|_| Ok(fixture.take())), Ok(Loaded::Fleet(diff)) if diff.files.is_empty())
        );
    }
}
