//! Debug capture only. A capture build cannot make a standalone upload that
//! NETT left unresolved: it talks to the fictional Subspace API, which keeps
//! no transfers, and adding a file needs the system file picker.
//! `SUBSPACE_CAPTURE_UPLOAD_SCENARIO` (`CAPTURE_UPLOAD_SCENARIO` in
//! `scripts/capture.sh`) makes a capture build against that loopback fixture
//! seed one invented upload on the first issue whose detail it reads, as a
//! NETT upload would have been left:
//!
//! - `unknown`: its bytes went to a reserved transfer and the reply was
//!   lost, so it waits for a check. A check against Subspace API cannot tell
//!   either, as when NETT finds a same-named file without size or digest.
//! - `absent`: kept while its issue was closed, its bytes were let go, and a
//!   check then found it was not stored.
//!
//! Everything after the seed is the app's own handling. Release builds never
//! contain it.
use super::{AttachmentUpload, UploadIssue};
use crate::hub::{AttachmentState, State};
use crate::uploads::{ImageDeclaration, UploadError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

const ENV: &str = "SUBSPACE_CAPTURE_UPLOAD_SCENARIO";
const FILE_NAME: &str = "fixture-checklist.txt";
const FILE: &[u8] = b"Fixture checklist\n";
/// The invented transfer the seeded upload reserved.
const TRANSFER: &str = "capture-transfer";

/// How the seeded upload was left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    /// Its bytes may be stored; it waits for a check.
    Unknown,
    /// Its bytes were let go, and a check found it was not stored.
    Absent,
}

impl Scenario {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "unknown" => Some(Self::Unknown),
            "absent" => Some(Self::Absent),
            _ => None,
        }
    }
}

/// Seeds the upload once, on the first issue open with its detail read, in
/// a capture against the loopback fixture with a scenario set.
pub(super) fn seed(state: &mut State) {
    static SEEDED: AtomicBool = AtomicBool::new(false);
    if SEEDED.load(Ordering::Relaxed) {
        return;
    }
    if let Some(scenario) = scenario()
        && seed_into(state, scenario)
    {
        SEEDED.store(true, Ordering::Relaxed);
    }
}

fn scenario() -> Option<Scenario> {
    static SCENARIO: OnceLock<Option<Scenario>> = OnceLock::new();
    *SCENARIO.get_or_init(|| {
        let api = std::env::var("SUBSPACE_API_URL").ok();
        let fixture = std::env::var_os("SUBSPACE_CAPTURE_DIR").is_some()
            && crate::backend::backend() == crate::Backend::Subspace
            && crate::fixture_from_env(true, api.as_deref()).is_some();
        fixture
            .then(|| std::env::var(ENV).ok())
            .flatten()
            .and_then(|value| Scenario::parse(&value))
    })
}

/// Adds the upload `scenario` describes to the open issue, once its detail
/// is read. Whether it did.
fn seed_into(state: &mut State, scenario: Scenario) -> bool {
    let Some(open) = state.snapshot.issue.as_ref() else {
        return false;
    };
    let (Some(detail), Some(upload)) = (open.detail.as_ref(), invented(scenario)) else {
        return false;
    };
    let issue = UploadIssue {
        id: detail.fields.id.clone(),
        identifier: open.identifier.clone(),
    };
    let opening = open.opening;
    state.attachment_buffers.push(Arc::downgrade(&upload.bytes));
    state.uploads.add(upload, issue, opening);
    true
}

/// The invented upload, as the app records it in `scenario`.
fn invented(scenario: Scenario) -> Option<AttachmentUpload> {
    let declaration = ImageDeclaration::new(FILE_NAME, "text/plain", FILE).ok()?;
    let mut upload = AttachmentUpload::new(declaration, FILE.into());
    upload.state = AttachmentState::Failed(UploadError::Checking);
    upload.transfer = Some(TRANSFER.to_owned());
    upload.bytes_sent = true;
    if scenario == Scenario::Absent {
        // As letting its bytes go and then a check finding nothing record it.
        upload.bytes = Arc::from(Vec::new());
        upload.released = true;
        upload.state = AttachmentState::NotFound;
        upload.transfer = None;
        upload.bytes_sent = false;
    }
    Some(upload)
}

#[cfg(test)]
mod tests {
    use super::{Scenario, invented, seed_into};
    use crate::hub::{AttachmentState, IssueSnapshot, IssueStatus, State};

    const ISSUE: &str = "00000000-0000-4000-8000-0000000000c1";

    fn state() -> State {
        State::new("http://127.0.0.1:1/api", "Subspace API")
    }

    /// Opens SUB-1 in `state`, with its detail read as `id`, if any.
    fn open(state: &mut State, id: Option<&str>) {
        let mut issue = IssueSnapshot::new("SUB-1", crate::PROFILE_ID, IssueStatus::Loaded);
        issue.detail = id.map(|id| serde_json::from_value(crate::tests::detail(id)).unwrap());
        state.snapshot.issue = Some(issue);
    }

    /// The open issue's uploads, as a snapshot shows them.
    fn shown(state: &State) -> usize {
        let open = state.open_for_uploads();
        state.uploads.shown(open.as_ref()).count()
    }

    #[test]
    fn only_the_two_scenarios_are_read() {
        assert_eq!(Scenario::parse("unknown"), Some(Scenario::Unknown));
        assert_eq!(Scenario::parse("absent"), Some(Scenario::Absent));
        assert_eq!(Scenario::parse(""), None);
        assert_eq!(Scenario::parse("Unknown"), None);
    }

    /// The upload waits for a check that can still change the outcome, and
    /// is not yet offered to be sent again.
    #[test]
    fn an_unknown_upload_waits_for_a_check() {
        let upload = invented(Scenario::Unknown).unwrap();
        assert!(matches!(upload.state(), AttachmentState::Failed(_)));
        assert!(upload.checks() && upload.retryable());
        assert!(!upload.released() && !upload.may_send_again());
    }

    /// The upload was let go and found not stored: nothing is left to check
    /// or send.
    #[test]
    fn an_absent_upload_was_let_go_and_not_stored() {
        let upload = invented(Scenario::Absent).unwrap();
        assert_eq!(upload.state(), &AttachmentState::NotFound);
        assert!(upload.released() && !upload.checks() && !upload.retryable());
    }

    /// It is seeded only on an issue whose detail is read, which then shows
    /// it.
    #[test]
    fn the_upload_is_seeded_on_the_open_issue_once_its_detail_is_read() {
        let mut state = state();
        assert!(!seed_into(&mut state, Scenario::Unknown));
        open(&mut state, None);
        assert!(!seed_into(&mut state, Scenario::Unknown));
        open(&mut state, Some(ISSUE));
        assert!(seed_into(&mut state, Scenario::Unknown));
        assert_eq!(shown(&state), 1);
    }
}
