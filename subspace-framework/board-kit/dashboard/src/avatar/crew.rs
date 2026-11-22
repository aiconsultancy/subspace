//! A crew meter: one dot for each member of an agent's crew, filled while
//! that member is busy.

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    div, px,
};

use crate::Tokens;

/// A dot's side, as the shared unread dot's.
const DOT_PX: f32 = 6.;
const GAP_PX: f32 = 3.;
/// The most dots a meter draws; a larger crew shows its count as text.
pub const MAX_DOTS: usize = 8;

/// `busy` of `total` crew: [`crew_meter`] builds it.
#[derive(IntoElement)]
pub struct CrewMeter {
    id: ElementId,
    busy: usize,
    total: usize,
}

/// A row of dots, one per crew member, the busy ones filled in green and
/// the idle ones hollow and muted, named *3 of 4 crew busy*. A crew larger
/// than [`MAX_DOTS`] shows `busy/total` in muted text instead.
pub fn crew_meter(id: impl Into<ElementId>, busy: usize, total: usize) -> CrewMeter {
    CrewMeter {
        id: id.into(),
        busy: busy.min(total),
        total,
    }
}

impl CrewMeter {
    /// The accessible name.
    pub fn spoken(&self) -> String {
        format!("{} of {} crew busy", self.busy, self.total)
    }

    /// Each dot in order, `true` for a busy member; none when the crew is
    /// too large for dots.
    fn dots(&self) -> Vec<bool> {
        if self.total > MAX_DOTS {
            return Vec::new();
        }
        (0..self.total).map(|index| index < self.busy).collect()
    }
}

/// One dot: filled for a busy member, a hollow ring for an idle one.
fn dot(busy: bool, tokens: &Tokens) -> AnyElement {
    let dot = div().size(px(DOT_PX)).flex_shrink_0().rounded_full();
    let dot = if busy {
        dot.bg(tokens.green)
    } else {
        dot.border_1().border_color(tokens.muted)
    };
    dot.into_any_element()
}

/// The meter's content: its dots, or its count for a large crew.
fn content(meter: &CrewMeter, tokens: &Tokens) -> Vec<AnyElement> {
    let dots = meter.dots();
    if dots.is_empty() && meter.total > 0 {
        let count = format!("{}/{}", meter.busy, meter.total);
        return vec![
            div()
                .text_color(tokens.muted)
                .child(count)
                .into_any_element(),
        ];
    }
    dots.into_iter().map(|busy| dot(busy, tokens)).collect()
}

impl RenderOnce for CrewMeter {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let children = content(&self, &tokens);
        h_flex()
            .id(self.id.clone())
            .test_support()
            .role(Role::Meter)
            .aria_label(self.spoken())
            .flex_shrink_0()
            .items_center()
            .gap(px(GAP_PX))
            .text_size(tokens.caption_size())
            .children(children)
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_DOTS, crew_meter};

    #[test]
    fn busy_members_lead_and_the_meter_says_how_many() {
        let meter = crew_meter("lyra", 3, 4);
        assert_eq!(meter.dots(), [true, true, true, false]);
        assert_eq!(meter.spoken(), "3 of 4 crew busy");
        assert_eq!(crew_meter("vega", 0, 4).dots(), [false; 4]);
        assert!(crew_meter("none", 0, 0).dots().is_empty());
    }

    #[test]
    fn busy_never_exceeds_the_crew_and_a_large_crew_has_no_dots() {
        let over = crew_meter("over", 6, 4);
        assert_eq!(over.spoken(), "4 of 4 crew busy");
        assert!(crew_meter("fleet", 3, MAX_DOTS + 1).dots().is_empty());
        assert_eq!(crew_meter("full", 2, MAX_DOTS).dots().len(), MAX_DOTS);
    }
}
