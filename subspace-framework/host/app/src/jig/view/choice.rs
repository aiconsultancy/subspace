//! The designer's choice tokens, such as a toolbar button finish.

use gpui_kit::component::h_flex;
use gpui_kit::{Context, Div, ParentElement as _};
use subspace_dashboard::Tokens;
use subspace_dashboard::density::{Control, Density};
use subspace_dashboard::segmented::{self, Segment};

use super::super::Jig;
use super::sections::labelled;

/// A token with named values as a segmented choice that previews live like
/// the sliders.
pub(super) fn density_choice(control: &'static Control, cx: &mut Context<Jig>) -> Div {
    let token = control.token;
    // A float cast saturates, so a negative value selects the first choice.
    let selected = cx.global::<Density>().get(token).round() as usize;
    let jig = cx.entity().downgrade();
    let segments = control
        .choices
        .iter()
        .map(|(_, label)| Segment::new(*label));
    labelled(
        control.label,
        h_flex().child(segmented::control(
            token,
            segments,
            Some(selected),
            &Tokens::current(cx),
            move |index, _, _, cx| {
                jig.update(cx, |jig, cx| jig.set_density(token, index as f32, cx))
                    .ok();
            },
        )),
    )
}
