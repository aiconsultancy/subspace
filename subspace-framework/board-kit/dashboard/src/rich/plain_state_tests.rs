use super::PlainState;
use gpui_kit::{AppContext as _, TestAppContext};
#[gpui_kit::test]
fn edits_clear_stale_ranges_and_invalid_utf8_boundaries_never_reach_shaping(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let state =
        cx.update(|cx| cx.new(|cx| PlainState::new("literal".into(), "59 minutes ago".into(), cx)));
    state.update(cx, |state, cx| {
        state.highlight(vec![(11..14, crate::Tokens::current(cx).accent)], cx);
        state.reveal(11..14, cx);
        state.set_text("é ago".into(), cx);
        assert!(state.highlights.is_empty());
        assert!(state.reveal.is_none());
        state.highlight(
            vec![
                (1..3, crate::Tokens::current(cx).accent),
                (3..6, crate::Tokens::current(cx).accent),
                (0..99, crate::Tokens::current(cx).accent),
            ],
            cx,
        );
        assert_eq!(state.highlights.len(), 1);
        assert_eq!(state.highlights[0].0, 3..6);
    });
}
