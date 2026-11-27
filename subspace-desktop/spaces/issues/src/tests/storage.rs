//! What the board keeps on disk (spec section 8): its choices in the state
//! file, never issue content, a search, a cursor or an identifier; nothing
//! beside the state file; and no item search over issues.

use gpui_kit::TestAppContext;
use subspace_framework::dashboard::{Dashboard as _, prefs};

use super::offline::{CURSOR, LATER_TITLE, TITLE, board_with_hub, offline, settle_board};
use crate::lines;
use crate::model::Tab;

const SEARCH: &str = "fixture secret words";
const GROUP: &str = "state-started";

#[gpui_kit::test]
fn a_new_install_starts_with_all_issues(cx: &mut TestAppContext) {
    let (board, cx) = super::mount_with_tab(cx, super::WIDTH, None);
    board.read_with(cx, |board, _| assert_eq!(board.tab, Tab::All));
}

#[gpui_kit::test]
fn board_state_keeps_choices_but_no_issue_content(cx: &mut TestAppContext) {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("state.json");
    let offline = offline();
    let (board, cx) = board_with_hub(&offline, cx);
    cx.update(|_, cx| prefs::init_file(Some(file.clone()), cx));
    settle_board(&board, cx, |board| board.listing.content.is_some());
    board.update(cx, |board, cx| board.read_more(GROUP, cx));
    settle_board(&board, cx, |board| {
        board
            .listing
            .content
            .as_ref()
            .is_some_and(|content| content.groups[0].rows.len() == 2)
    });
    board.update(cx, |board, cx| {
        board.activate_line(&lines::group_key(GROUP), cx);
        board.query = SEARCH.into();
        board.read_list(cx);
        board.set_tab(Tab::All, cx);
    });
    cx.update(|_, cx| prefs::save(cx)).unwrap();
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(
        saved.contains("issues.tab") && saved.contains("issues.folded"),
        "the board's choices are saved: {saved}"
    );
    for forbidden in [TITLE, LATER_TITLE, CURSOR, SEARCH, "issue:1", "SUB-1"] {
        assert!(!saved.contains(forbidden), "{forbidden} reached {saved}");
    }
    let files: Vec<_> = std::fs::read_dir(folder.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(files, ["state.json"], "no cache beside the state file");
    cx.update(|_, cx| assert!(board.read(cx).search(cx).is_none(), "no item search"));
}
