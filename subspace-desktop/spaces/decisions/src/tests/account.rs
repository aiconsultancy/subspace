//! A switch of account or organisation, or a sign-out, drops everything
//! read for the previous account, including answers still in flight.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_api_client::Error;
use subspace_framework::dashboard::{Dashboard as _, ReturnLocation};

use crate::fixtures::{Accounts, OTHER_DECISION, OTHER_NAME, Who, reader_for};
use crate::load::{Leased, Plan, Reader};
use crate::tests::{has, label, mount_with, press, render, rows, select, selected};
use crate::{Decisions, menus, page, search};

/// Lets the Space's account watch run.
fn watch(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_secs(1));
    render(cx);
}

fn return_to(
    board: &Entity<Decisions>,
    place: &ReturnLocation,
    cx: &mut VisualTestContext,
) -> bool {
    let returned = cx
        .update(|window, cx| board.update(cx, |board, cx| board.return_to(&place.key, window, cx)));
    render(cx);
    returned
}

/// Reads with `reader` from now on, and reloads.
fn reload_with(board: &Entity<Decisions>, reader: Reader, cx: &mut VisualTestContext) {
    board.update(cx, |board, _| board.reader = reader);
    cx.update(|window, cx| board.update(cx, |board, cx| board.reload(window, cx)));
    render(cx);
}

fn tags(board: &Entity<Decisions>, cx: &mut VisualTestContext) -> BTreeSet<String> {
    cx.update(|_, cx| board.read(cx).session.tags.clone())
}

fn account(board: &Entity<Decisions>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| board.read(cx).session.account.clone())
}

/// The list as the other account sees it, and nothing of the operator's.
fn shows_the_other_account(board: &Entity<Decisions>, cx: &mut VisualTestContext) {
    assert_eq!(rows(board, cx), [format!("decision:{OTHER_DECISION}")]);
    assert_eq!(account(board, cx).as_deref(), Some(OTHER_NAME));
    assert_eq!(tags(board, cx), BTreeSet::from(["ops".to_owned()]));
    assert!(cx.update(|_, cx| board.read(cx).page.is_none()));
    assert!(!has(cx, "decisions-page"));
}

#[gpui_kit::test]
fn switching_account_drops_everything_read_for_the_previous_one(cx: &mut TestAppContext) {
    let accounts = Accounts::default();
    let (board, cx) = mount_with(reader_for(&accounts), cx);
    board.update(cx, |board, cx| board.choose_tag(Some("nav".to_owned()), cx));
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            board
                .search
                .update(cx, |input, cx| input.set_value("relay", window, cx));
            search::commit(board, cx);
        });
    });
    render(cx);
    select(&board, "d2", cx);
    press(cx, "enter");
    assert!(has(cx, "decisions-page"));
    let page_place = cx
        .update(|_, cx| board.read(cx).return_location(cx))
        .unwrap();
    // The list's tab again: the decision stays selected and its tab open.
    board.update(cx, |board, cx| board.show_tab(0, cx));
    render(cx);
    let row_place = cx
        .update(|_, cx| page::issue_origin(board.read(cx)))
        .unwrap();
    assert_eq!(selected(&board, cx).as_deref(), Some("decision:d2"));

    accounts.switch(Who::Other);
    watch(cx);

    shows_the_other_account(&board, cx);
    assert_eq!(selected(&board, cx), None);
    let (tag, query, field) = cx.update(|_, cx| {
        let board = board.read(cx);
        (
            board.choice.tag.clone(),
            board.query.clone(),
            board.search.read(cx).value().to_string(),
        )
    });
    assert_eq!(tag, None, "the previous account's tag is not kept");
    assert!(query.is_empty() && field.is_empty(), "nor its search");
    // The other account's directory names the account and its projects.
    assert_eq!(
        label(cx, "decisions-account").as_deref(),
        Some("Signed in as second-operator")
    );
    assert!(has(cx, "decisions-project:project:OPS"));
    assert!(!has(cx, "decisions-project:project:NAV"));
    // A Back to a place shown for the previous account no longer returns.
    assert!(!return_to(&board, &page_place, cx));
    assert!(!return_to(&board, &row_place, cx));
    assert!(!has(cx, "decisions-page"));
    assert_eq!(selected(&board, cx), None);
}

#[gpui_kit::test]
fn an_answer_read_for_the_previous_account_is_dropped(cx: &mut TestAppContext) {
    let accounts = Accounts::default();
    let (board, cx) = mount_with(reader_for(&accounts), cx);
    board.update(cx, |board, cx| board.open_decision("d2", cx));
    render(cx);
    assert!(has(cx, "decisions-page"));
    // The next list read asks as the operator; the account changes before
    // its answer lands.
    let mut reader = reader_for(&accounts);
    let (operator, during) = (reader.list.clone(), accounts.clone());
    let first = Arc::new(AtomicBool::new(true));
    reader.list = Arc::new(move |plan: &Plan| {
        let answer = operator(plan);
        if first.swap(false, Ordering::SeqCst) {
            during.switch(Who::Other);
        }
        answer
    });
    reload_with(&board, reader, cx);

    shows_the_other_account(&board, cx);
}

#[gpui_kit::test]
fn a_read_that_can_make_no_client_keeps_watching_the_shown_account(cx: &mut TestAppContext) {
    let accounts = Accounts::default();
    let (board, cx) = mount_with(reader_for(&accounts), cx);
    let shown = rows(&board, cx);
    assert!(!shown.is_empty());
    // The next list read can make no client, so it reads nothing for any
    // account and refuses.
    let mut reader = reader_for(&accounts);
    let operator = reader.list.clone();
    let first = Arc::new(AtomicBool::new(true));
    reader.list = Arc::new(move |plan: &Plan| {
        if first.swap(false, Ordering::SeqCst) {
            return Leased::Unread(Error::InvalidRequest);
        }
        operator(plan)
    });
    reload_with(&board, reader, cx);
    assert_eq!(
        rows(&board, cx),
        shown,
        "an unexpected failure keeps the list"
    );

    // What stays was read for the operator, so a switch still drops it.
    accounts.switch(Who::Other);
    watch(cx);

    shows_the_other_account(&board, cx);
}

#[gpui_kit::test]
fn the_frame_that_finds_the_account_changed_draws_no_previous_search_or_menu(
    cx: &mut TestAppContext,
) {
    let accounts = Accounts::default();
    let (board, cx) = mount_with(reader_for(&accounts), cx);
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            board
                .search
                .update(cx, |input, cx| input.set_value("relay", window, cx));
            menus::open(board, menus::Kind::Tag, window, cx);
        });
    });
    render(cx);
    assert!(cx.update(|_, cx| board.read(cx).menu.is_some()));

    // The account changes, and a frame is drawn before the account watch
    // next runs.
    accounts.switch(Who::Other);
    board.update(cx, |_, cx| cx.notify());
    render(cx);

    let (field, menu, host) = cx.update(|_, cx| {
        let board = board.read(cx);
        (
            board.search.read(cx).value().to_string(),
            board.menu.is_some(),
            board.menu_anchors.view().is_some(),
        )
    });
    assert!(field.is_empty(), "the previous account's search is cleared");
    assert!(!menu && !host, "and its menu closed");
}

#[gpui_kit::test]
fn signing_out_drops_everything_read_for_the_account(cx: &mut TestAppContext) {
    let accounts = Accounts::default();
    let (board, cx) = mount_with(reader_for(&accounts), cx);
    select(&board, "d2", cx);
    press(cx, "enter");
    assert!(has(cx, "decisions-page"));

    accounts.switch(Who::SignedOut);
    watch(cx);

    assert!(!has(cx, "decisions-page"));
    assert!(cx.update(|_, cx| board.read(cx).page.is_none()));
    assert!(rows(&board, cx).is_empty());
    assert_eq!(selected(&board, cx), None);
    assert!(tags(&board, cx).is_empty());
    assert_eq!(account(&board, cx), None);
    assert_eq!(
        label(cx, "decisions-state").as_deref(),
        Some("Sign-in required")
    );
    let status = cx.update(|_, cx| board.read(cx).status(cx)).unwrap();
    assert_eq!(status.as_ref(), "Subspace API · Sign-in required");
}

#[gpui_kit::test]
fn a_hidden_space_drops_the_previous_account_and_reads_when_shown(cx: &mut TestAppContext) {
    let accounts = Accounts::default();
    let (board, cx) = mount_with(reader_for(&accounts), cx);
    cx.update(|window, cx| board.update(cx, |board, cx| board.deactivate(window, cx)));
    let earlier = cx.update(|_, cx| board.read(cx).stop.clone());

    accounts.switch(Who::Other);
    watch(cx);

    assert!(rows(&board, cx).is_empty());
    assert_eq!(account(&board, cx), None);
    assert!(
        cx.update(|_, cx| !board.read(cx).listing.loading()),
        "nothing is read while hidden"
    );
    assert!(earlier.stopped(), "a read for the previous account stops");
    cx.update(|window, cx| board.update(cx, |board, cx| board.activate(window, cx)));
    render(cx);
    shows_the_other_account(&board, cx);
}

#[gpui_kit::test]
fn a_newer_read_stops_the_one_it_supersedes(cx: &mut TestAppContext) {
    let accounts = Accounts::default();
    let (board, cx) = mount_with(reader_for(&accounts), cx);
    let older = board.update(cx, |board, cx| {
        board.read(false, cx);
        let older = board.stop.clone();
        board.read(false, cx);
        older
    });
    assert!(
        older.stopped(),
        "the superseded read asks for no more pages"
    );
    assert!(cx.update(|_, cx| !board.read(cx).stop.stopped()));
    render(cx);
    assert_eq!(rows(&board, cx).len(), 5);
}
