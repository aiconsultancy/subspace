//! Every form builder, called directly, captions each of its states.

use gpui_kit::TestAppContext;

use super::{ITEMS, fields, pickers};
use crate::samples::tests::{captions, every_state_is_captioned};

#[gpui_kit::test]
fn form_builders_caption_every_state(cx: &mut TestAppContext) {
    let items = captions(cx, |g, t, w, cx| {
        vec![
            fields::inputs(g, t, w, cx),
            fields::textareas(g, t, w, cx),
            fields::editors(g, t, w, cx),
            fields::selects(g, t, w, cx),
            fields::comboboxes(g, t, w, cx),
            fields::number_inputs(g, t, w, cx),
            pickers::date_pickers(g, t, w, cx),
            pickers::time_fields(g, t, w, cx),
            fields::otp_inputs(g, t, w, cx),
            pickers::speech_buttons(g, t, w, cx),
            pickers::color_pickers(g, t, w, cx),
            fields::forms(g, t, w, cx),
            fields::input_groups(g, t, w, cx),
        ]
    });
    assert_eq!(items.len(), ITEMS.len());
    assert!(every_state_is_captioned(&items));
}
