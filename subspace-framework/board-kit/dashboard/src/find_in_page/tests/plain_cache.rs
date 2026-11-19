//! Plain text revisions invalidate the native paint cache even after an A->B->A edit.
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

#[gpui_kit::test]
fn plain_text_changing_back_before_rebuild_still_reapplies_highlights(cx: &mut TestAppContext) {
    use crate::find_in_page::participant::Native;
    let (pane, cx) = crate::find_in_page::test_support::mount(cx);
    cx.update(|window, cx| window.press(crate::secondary!("f"), cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.input("needle", cx));
    cx.run_until_parked();
    let plain = pane.read_with(cx, |p, cx| {
        p.session
            .read(cx)
            .entries
            .values()
            .find_map(|entry| match &entry.native {
                Native::Plain(state) => state.upgrade(),
                _ => None,
            })
            .unwrap()
    });
    assert!(plain.read_with(cx, |s, _| s.highlight_count()) > 0);
    plain.update(cx, |s, cx| {
        let original = s.text.clone();
        s.set_text("other".into(), cx);
        s.set_text(original, cx);
        assert_eq!(s.highlight_count(), 0);
    });
    cx.run_until_parked();
    assert!(plain.read_with(cx, |s, _| s.highlight_count()) > 0);
}
