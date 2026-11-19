use gpui_kit::TestAppContext;
use subspace_core::features::PI_CHAT;

use super::{assert_dirty, open_settings};

#[gpui_kit::test]
fn service_fields_new_credentials_and_clear_choices_are_reversible(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            let field = settings.form.repos.clone();
            field.update(cx, |list, cx| list.toggle("test-only/edit", cx));
            assert_dirty(settings, true, cx);
            field.update(cx, |list, cx| list.toggle("test-only/edit", cx));
            assert_dirty(settings, false, cx);
            for index in 0..settings.form.tokens.len() {
                let field = &settings.form.tokens[index].input;
                // Readable, non-working fixture, never a saved credential.
                field.update(cx, |field, cx| {
                    field.set_value("test-only-nonworking-credential", window, cx)
                });
                assert_dirty(settings, true, cx);
                field.update(cx, |field, cx| field.set_value("", window, cx));
                assert_dirty(settings, false, cx);
                settings.toggle_clear(index, window, cx);
                assert_dirty(settings, true, cx);
                settings.toggle_clear(index, window, cx);
                assert_dirty(settings, false, cx);
            }
        })
        .unwrap();
}

#[gpui_kit::test]
fn board_and_feature_drafts_compare_effective_values_not_explicit_overrides(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.form.set_board_enabled("future", false);
            assert_dirty(settings, true, cx);
            settings.form.set_board_enabled("future", true);
            assert_dirty(settings, false, cx);
            settings.toggle_feature(&PI_CHAT, true, window, cx);
            assert_dirty(settings, true, cx);
            settings.toggle_feature(&PI_CHAT, false, window, cx);
            assert_dirty(settings, false, cx);
            settings.move_board("future", 1, cx);
            assert_dirty(settings, true, cx);
            settings.move_board("future", -1, cx);
            assert_dirty(settings, false, cx);
        })
        .unwrap();
}
