use super::{TabSpec, paint::state};
use crate::{Tokens, tokens::ButtonFinish};
use gpui_kit::FontWeight;

#[test]
fn selection_joins_the_content_and_hover_preserves_the_accent() {
    let tokens = Tokens::one_dark();
    let selected = state(true, false, false, &tokens, tokens.bg);
    assert_eq!(selected.background, tokens.bg);
    assert_eq!(selected.foreground, tokens.fg_strong);
    assert_eq!(selected.edge, tokens.accent);
    assert_eq!(selected.weight, FontWeight::MEDIUM);
    assert_eq!(state(true, true, false, &tokens, tokens.bg), selected);
    let idle = state(false, false, false, &tokens, tokens.bg);
    assert_eq!(idle.background.a, 0.);
    assert_eq!(idle.foreground, tokens.muted);
    assert_eq!(idle.edge.a, 0.);
}

#[test]
fn hover_follows_the_flat_or_glass_finish() {
    let mut tokens = Tokens::one_dark();
    assert_eq!(
        state(false, true, false, &tokens, tokens.bg).background,
        tokens.button_hover
    );
    tokens.controls.toolbar_look.hover = ButtonFinish::Glass;
    assert_eq!(
        state(false, true, false, &tokens, tokens.bg).background,
        tokens.glass
    );
}

#[test]
fn unread_items_have_a_separate_accessible_state() {
    let tab = TabSpec::plain("#ops").unread(true);
    assert_eq!(tab.accessible_label(), "#ops, unread items");
    assert_eq!(TabSpec::plain("#ops").accessible_label(), "#ops");
}

#[test]
fn disabled_tabs_stay_muted_without_an_accent_in_every_state() {
    let tokens = Tokens::one_dark();
    for selected in [false, true] {
        for hovered in [false, true] {
            let disabled = state(selected, hovered, true, &tokens, tokens.bg);
            assert_eq!(disabled.foreground, tokens.muted);
            assert_eq!(disabled.weight, FontWeight::NORMAL);
            assert_eq!(disabled.edge.a, 0.);
            assert_eq!(disabled.background.a, 0.);
        }
    }
}
