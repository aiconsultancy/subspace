//! An open issue's uploads, kept aside while it is closed: a transfer may
//! finish, or need a check, later.
use super::AttachmentUpload;

impl crate::hub::State {
    /// Keeps the open issue's uploads aside as it closes or another issue
    /// opens in its place: a transfer may finish, or need a check, later.
    pub(in crate::hub) fn park_open_uploads(&mut self) {
        let Some(issue) = self.snapshot.issue.as_mut() else {
            return;
        };
        let uploads = std::mem::take(&mut issue.files.uploads);
        if !uploads.is_empty() {
            self.parked_uploads
                .entry(issue.identifier.clone())
                .or_default()
                .extend(uploads);
        }
    }

    /// Shows the uploads kept for `issue` again as it opens.
    pub(in crate::hub) fn unpark_uploads(&mut self, issue: &mut crate::hub::IssueSnapshot) {
        if let Some(uploads) = self.parked_uploads.remove(&issue.identifier) {
            issue.files.uploads.extend(uploads);
        }
    }

    /// The upload `id` of `issue`, whether the issue is open or its uploads
    /// were kept aside when it closed.
    pub(super) fn upload_row(&mut self, issue: &str, id: &str) -> Option<&mut AttachmentUpload> {
        let open = self
            .snapshot
            .issue
            .as_mut()
            .filter(|open| open.identifier == issue)
            .map(|open| &mut open.files.uploads);
        let uploads = match open {
            Some(uploads) => uploads,
            None => self.parked_uploads.get_mut(issue)?,
        };
        uploads.iter_mut().find(|upload| upload.id == id)
    }

    /// Forgets the upload `id` of `issue` once it is stored.
    pub(super) fn remove_upload(&mut self, issue: &str, id: &str) {
        if let Some(open) = self
            .snapshot
            .issue
            .as_mut()
            .filter(|open| open.identifier == issue)
        {
            open.files.uploads.retain(|upload| upload.id != id);
        }
        if let Some(parked) = self.parked_uploads.get_mut(issue) {
            parked.retain(|upload| upload.id != id);
            if parked.is_empty() {
                self.parked_uploads.remove(issue);
            }
        }
    }
}

#[cfg(test)]
mod parking_tests {
    use crate::hub::{AttachmentState, Hub, IssueSnapshot, IssueStatus, lock};
    use crate::uploads::UploadError;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use subspace_framework::client_test_support::{Mock, Request};

    const UPLOAD: &str = "upload-parked-on-close";

    /// NETT serving attachments: its capability list, the reservation and
    /// the file host. The test answers completion itself.
    fn nett(base: &Arc<Mutex<String>>, request: &Request) -> (u16, String) {
        let path = request.path.as_str();
        if path.ends_with("/desktop/capabilities") {
            (200, r#"{"features":["attachments"]}"#.into())
        } else if path.ends_with("/attachments/prepare") {
            let url = format!("{}/files/{UPLOAD}", lock(base));
            let reply = serde_json::json!({"uploadId": UPLOAD, "uploadUrl": url, "method": "PUT"});
            (200, reply.to_string())
        } else if request.method == "PUT" && path.ends_with(UPLOAD) {
            (200, String::new())
        } else {
            (404, "{}".into())
        }
    }

    fn wait_for(what: &str, done: &dyn Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn an_upload_in_progress_when_its_issue_closes_is_shown_again_when_it_reopens() {
        let base = Arc::new(Mutex::new(String::new()));
        let served = Arc::clone(&base);
        let mock = Mock::with_headers(move |request| {
            if request.path.ends_with("/attachments/from-upload") {
                std::thread::sleep(Duration::from_millis(400));
                return (202, vec![("Retry-After".into(), "120".into())], "{}".into());
            }
            let (status, body) = nett(&served, request);
            (status, Vec::new(), body)
        });
        *lock(&base) = mock.base_url().to_owned();
        let client = crate::Client::nett_loopback(
            &format!("{}/api", mock.base_url()),
            Some("00000000-0000-4000-8000-0000000000f1"),
            Some(subspace_framework::core::Secret::new("nett-parked-upload")),
        )
        .unwrap()
        .without_live();
        let hub = Hub::from_client(client, "NETT");
        {
            // The issue is open with its detail, as once it has loaded.
            let mut state = lock(&hub.0.state);
            let mut issue = IssueSnapshot::new("OPS-1", crate::PROFILE_ID, IssueStatus::Loaded);
            issue.detail = Some(serde_json::from_value(crate::tests::detail("1")).unwrap());
            state.snapshot.issue = Some(issue);
        }
        hub.add_attachment(
            "parked.txt",
            "text/plain",
            b"Checked after the issue closed.\n".to_vec(),
        )
        .unwrap();
        // The bytes reached the file host and completion is under way when
        // the person moves away from the issue.
        wait_for("the completion", &|| {
            mock.requests()
                .iter()
                .any(|request| request.path.ends_with("/attachments/from-upload"))
        });
        hub.close_issue();
        wait_for("the outcome, kept aside", &|| {
            lock(&hub.0.state)
                .parked_uploads
                .get("OPS-1")
                .is_some_and(|uploads| matches!(uploads[0].state, AttachmentState::Failed(_)))
        });
        hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
        let snapshot = hub.snapshot();
        let uploads = snapshot.issue().unwrap().files().uploads();
        assert_eq!(uploads.len(), 1, "the upload is shown again");
        assert!(uploads[0].checks(), "Check completes the same transfer");
        assert!(matches!(
            uploads[0].state(),
            AttachmentState::Failed(UploadError::Unclear { retry_after: 120 })
        ));
        assert!(lock(&hub.0.state).parked_uploads.is_empty());
        hub.stop();
    }

    #[test]
    fn uploads_kept_aside_are_never_shown_to_another_account() {
        let mut state = crate::hub::State::new("http://127.0.0.1:1/api", "NETT");
        state.parked_uploads.insert(
            "OPS-1".into(),
            vec![super::super::admission_tests::upload(8)],
        );
        state.snapshot.me = Some(crate::model::Me {
            user_id: "first".into(),
            kind: "human".into(),
            display_name: "Fixture".into(),
        });
        assert!(state.retire_if_account_changed(&Ok("second".into()), None));
        assert!(state.parked_uploads.is_empty());
    }
}
