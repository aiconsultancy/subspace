use std::sync::{Arc, Mutex};

use gpui_kit::TestAppContext;
use subspace_api_client::decision_writes::{DecisionReview, DecisionWriteError};
use subspace_api_client::model::DecisionStatus;

use super::{Job, Mode, Problem};
use crate::load::Leased;
use crate::model::ProjectChoice;
use crate::tests::{click, has, label, mount, render};
use crate::{Decisions, fixtures};

#[gpui_kit::test]
fn a_proposal_opens_its_own_page_after_saving(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    board.update(cx, |board, _| {
        board.choice.project = ProjectChoice::Project("WARP".into());
        board.test_writer = Some(Arc::new(|job| {
            let Job::Create {
                project,
                fields,
                attribution,
            } = job
            else {
                panic!("expected a proposal");
            };
            assert_eq!(project, "project-WARP");
            assert_eq!(
                fields.changes.title.as_deref(),
                Some("Agree on relay timing")
            );
            assert_eq!(
                fields.changes.decision.as_deref(),
                Some("Read after the correction.")
            );
            assert!(attribution.is_none());
            let mut detail = fixtures::detail("d1").unwrap();
            detail.item.title = "Agree on relay timing".into();
            Ok(detail)
        }));
    });
    render(cx);
    click(cx, "decisions-new");
    assert!(has(cx, "decision-author-dialog"));
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            let Mode::Draft { draft, .. } = &board.author.as_ref().unwrap().mode else {
                panic!("expected a draft");
            };
            draft.title.update(cx, |field, cx| {
                field.set_value("Agree on relay timing", window, cx)
            });
            draft.decision.update(cx, |field, cx| {
                field.set_value("Read after the correction.", window, cx)
            });
        })
    });
    click(cx, "decision-author-submit");
    assert!(!has(cx, "decision-author-dialog"));
    assert_eq!(
        label(cx, "decisions-page-heading").as_deref(),
        Some("Agree on relay timing")
    );
}

#[gpui_kit::test]
fn a_conflict_keeps_the_draft_and_uses_a_version_only_after_review(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    board.update(cx, |board, cx| board.open_decision("d1", cx));
    render(cx);
    let versions = Arc::new(Mutex::new(Vec::new()));
    let observed = versions.clone();
    board.update(cx, |board, _| conflict_fixture(board, observed));
    click(cx, "decision-edit");
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            let Mode::Draft { draft, .. } = &board.author.as_ref().unwrap().mode else {
                panic!("expected a draft");
            };
            draft.title.update(cx, |field, cx| {
                field.set_value("Keep the draft title", window, cx)
            });
            draft.issues.update(cx, |field, cx| {
                field.set_value("WARP-8, WARP-9", window, cx)
            });
        })
    });
    click(cx, "decision-author-submit");
    assert_eq!(*versions.lock().unwrap(), [1]);
    assert!(has(cx, "decision-author-dialog"));
    cx.update(|window, cx| board.update(cx, |board, cx| board.use_current(window, cx)));
    render(cx);
    assert_eq!(
        *versions.lock().unwrap(),
        [1],
        "current version does not send a write"
    );
    click(cx, "decision-author-submit");
    assert_eq!(*versions.lock().unwrap(), [1, 2]);
    assert_eq!(
        label(cx, "decisions-page-heading").as_deref(),
        Some("Keep the draft title")
    );
}

fn conflict_fixture(board: &mut Decisions, versions: Arc<Mutex<Vec<u32>>>) {
    let read = board.reader.detail.clone();
    board.reader.detail = Arc::new(move |id, lease| match read(id, lease) {
        Leased::Read(lease, Ok(mut detail)) => {
            detail.item.version = Some(2);
            detail.item.revision_number = 2;
            detail.item.issues.count = 51;
            detail.context = "Changed by another fixture editor.".into();
            Leased::Read(lease, Ok(detail))
        }
        other => other,
    });
    board.test_writer = Some(Arc::new(move |job| {
        let Job::Edit { fields, target, .. } = job else {
            panic!("expected an edit");
        };
        versions.lock().unwrap().push(target.version);
        assert_eq!(
            fields.changes.title.as_deref(),
            Some("Keep the draft title")
        );
        assert!(
            fields.changes.context.is_none(),
            "keep the other editor’s context"
        );
        if target.version == 1 {
            assert!(fields.issues.is_some());
            return Err(Problem::Write(DecisionWriteError::Conflict));
        }
        assert!(
            fields.issues.is_none(),
            "preserve links the API did not return"
        );
        let mut detail = fixtures::detail("d1").unwrap();
        detail.item.title = fields.changes.title.unwrap();
        Ok(detail)
    }));
}
#[gpui_kit::test]
fn reviewing_a_proposal_requires_confirmation_and_removes_write_actions(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    board.update(cx, |board, cx| {
        board.open_decision("d1", cx);
        board.test_writer = Some(Arc::new(|job| {
            let Job::Review {
                target,
                review: DecisionReview::Accept,
            } = job
            else {
                panic!("expected acceptance");
            };
            assert_eq!(target.version, 1);
            let mut detail = fixtures::detail("d1").unwrap();
            detail.item.status = DecisionStatus::Accepted;
            Ok(detail)
        }));
    });
    render(cx);
    click(cx, "decision-accept");
    assert_eq!(
        label(cx, "decision-author-dialog").as_deref(),
        Some("Accept decision?")
    );
    click(cx, "decision-author-submit");
    assert!(!has(cx, "decision-edit"));
    assert!(!has(cx, "decision-accept"));
    assert!(!has(cx, "decision-reject"));
    assert!(has(cx, "decision-history"));
}
