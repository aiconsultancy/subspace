use super::{choose, click, has, normal_fixture, open, open_row, settle_detail, settle_page};
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn the_ninth_history_entry_evicts_the_oldest_and_back_still_works(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    open_row(&board, cx);
    for n in 2..=9 {
        let id = history_id(n);
        cx.update(|window, cx| {
            board.update(cx, |board, cx| {
                crate::decision::follow_link(
                    board,
                    crate::decision::Entry {
                        id: id.clone(),
                        title: format!("Choice {n}"),
                    },
                    window,
                    cx,
                );
            })
        });
        settle_detail(&board, cx, &id);
    }
    for n in (2..=8).rev() {
        assert!(has(cx, "decision-panel-back"));
        click("decision-panel-back", cx);
        let id = history_id(n);
        settle_detail(&board, cx, &id);
        cx.update(|_, cx| {
            assert_eq!(
                board
                    .read(cx)
                    .decision
                    .as_ref()
                    .unwrap()
                    .stack
                    .last()
                    .unwrap()
                    .id,
                id
            )
        });
    }
    assert!(!has(cx, "decision-panel-back"));
}

fn history_id(n: u8) -> String {
    format!("00000000-0000-0000-0000-{n:012}")
}
