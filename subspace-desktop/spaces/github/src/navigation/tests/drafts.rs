//! An open review draft survives the actions that could replace it, and never
//! carries over to another account.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::dashboard::Dashboard as _;

use super::{KEY, URL, mount, seed};
use crate::detail::Mode;
use crate::{Github, sample};

const OTHER_KEY: &str = "octo-org/api#415";

/// Opens the seeded pull request's page and starts a comment with `text`.
fn draft(board: &Entity<Github>, text: &'static str, cx: &mut VisualTestContext) {
    board.update(cx, seed);
    board.update_in(cx, |board, window, cx| {
        assert!(board.open_link(URL, cx));
        board.compose(Mode::Comment, window, cx);
        let composer = board.composer.as_ref().expect("the composer opens");
        composer
            .entity
            .update(cx, |composer, cx| composer.set_text(text, window, cx));
    });
}

fn draft_text(
    board: &Entity<Github>,
    cx: &mut VisualTestContext,
) -> Option<(String, Mode, String)> {
    board.read_with(cx, |board, cx| {
        board.composer.as_ref().map(|composer| {
            (
                composer.key.clone(),
                composer.mode,
                composer.entity.read(cx).text(cx),
            )
        })
    })
}

#[gpui_kit::test]
fn an_account_switch_drops_the_open_review_draft(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    draft(&board, "Looks right to me", cx);
    assert!(draft_text(&board, cx).is_some());
    board.update(cx, |board, cx| {
        board.set_account("other-account".into(), cx)
    });
    assert!(draft_text(&board, cx).is_none());
}

#[gpui_kit::test]
fn the_same_account_keeps_the_open_review_draft(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    draft(&board, "Looks right to me", cx);
    board.update(cx, |board, cx| board.set_account("OCTO-DEV".into(), cx));
    assert_eq!(
        draft_text(&board, cx),
        Some((KEY.into(), Mode::Comment, "Looks right to me".into()))
    );
}

#[gpui_kit::test]
fn a_review_action_on_the_same_pull_request_keeps_the_draft(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    draft(&board, "Ship it once the tests pass", cx);
    let before = board.read_with(cx, |board, _| {
        board.composer.as_ref().unwrap().entity.entity_id()
    });
    board.update_in(cx, |board, window, cx| {
        board.compose(Mode::Approve, window, cx)
    });
    assert_eq!(
        draft_text(&board, cx),
        Some((
            KEY.into(),
            Mode::Approve,
            "Ship it once the tests pass".into()
        ))
    );
    let after = board.read_with(cx, |board, _| {
        board.composer.as_ref().unwrap().entity.entity_id()
    });
    assert_eq!(before, after, "the same composer stays open");
}

#[gpui_kit::test]
fn a_review_action_on_another_pull_request_keeps_the_draft(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    draft(&board, "Half-written review", cx);
    board.update_in(cx, |board, window, cx| {
        let other = sample::items()
            .into_iter()
            .find(|item| item.key == OTHER_KEY)
            .unwrap();
        let section = board.section;
        let account = board.data.get_mut("octo-dev").unwrap();
        account.sections.get_mut(section).unwrap().items.push(other);
        board.rebuild(cx);
        assert!(board.list.reveal(OTHER_KEY));
        board.compose(Mode::Comment, window, cx);
    });
    assert_eq!(
        draft_text(&board, cx),
        Some((KEY.into(), Mode::Comment, "Half-written review".into()))
    );
}
