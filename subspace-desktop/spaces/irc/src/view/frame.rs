use gpui_kit::component::v_flex;
use gpui_kit::{Context, IntoElement, ParentElement as _, Render, Styled as _, Window, div};

use super::{bind_hub, empty};
use crate::{Irc, initiatives, join};

impl Render for Irc {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        bind_hub(self, cx);
        let content = match &self.hub {
            None => empty::stopped(cx),
            Some(hub) if hub.read(cx).state.networks().is_empty() => empty::unconfigured(self, cx),
            Some(_) => self.dock.area.clone().into_any_element(),
        };
        v_flex()
            .relative()
            .size_full()
            .child(initiatives::heading(self, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .px(initiatives::SPACE_INSET)
                    .pb_2()
                    .child(content)
                    .child(subspace_framework::dashboard::measure::height(
                        self.available_height.clone(),
                    )),
            )
            .children(self.initiative_menu.host())
            .children(join::render(self, cx))
    }
}
