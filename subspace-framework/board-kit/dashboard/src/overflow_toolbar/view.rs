use super::{OverflowAnchor, OverflowToolbar, OverflowUnit};
use crate::icon_button::{IconButton, Metrics};
use crate::workspace::FOCUS_RING_PX;
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::{Sizable as _, button::ButtonVariants as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, AvailableSpace, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Rems, Render, Styled as _, TestSupportExt as _, Window, div,
    px, size,
};
use subspace_lucide::IconName;

impl OverflowToolbar {
    // Layout-only probes never prepaint: hidden controls register no hitboxes or focus nodes.
    fn measure_units(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.widths = self
            .units
            .iter()
            .map(|unit| {
                let mut element = (unit.inline)(window, cx);
                let measured = element.layout_as_root(
                    size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
                    window,
                    cx,
                );
                f32::from(measured.width)
            })
            .collect();
    }

    fn hidden_units(&self) -> Vec<OverflowUnit> {
        let plan = self.plan();
        self.units[plan.visible..].to_vec()
    }

    fn refresh_popup(
        &mut self,
        anchor: &Entity<crate::anchored::Anchor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !std::mem::take(&mut self.popup_dirty) {
            return;
        }
        let Some(popup) = self.popup.as_ref().and_then(|popup| popup.upgrade()) else {
            return;
        };
        let units = self.hidden_units();
        let anchor = OverflowAnchor {
            bounds: self.trigger_bounds,
            host: anchor.clone(),
        };
        let (changed, items) = self.projection.build(&units, anchor);
        popup.update(cx, |popup, cx| {
            if changed {
                popup.rebuild(window, cx, |menu, _, _| Self::popup(menu, items));
            } else {
                cx.notify();
            }
        });
    }

    fn more_button(
        &self,
        anchor: Entity<crate::anchored::Anchor>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.weak_entity();
        let measured = entity.clone();
        div()
            .id("overflow-toolbar-more")
            .test_support()
            .flex_shrink_0()
            .w(px(self.chevron_width))
            .on_prepaint(move |bounds, _, cx| {
                _ = measured.update(cx, |toolbar, cx| {
                    if toolbar.trigger_bounds != bounds {
                        toolbar.trigger_bounds = bounds;
                        toolbar.popup_dirty = true;
                        cx.notify();
                    }
                });
            })
            .child(
                crate::anchored::dropdown_menu(
                    IconButton::new("overflow-toolbar-button")
                        .ghost()
                        .small()
                        .icon(IconName::ChevronDown)
                        .w(px(self.chevron_width))
                        .tooltip("More toolbar actions")
                        .accessibility_label("More toolbar actions"),
                    {
                        let anchor = anchor.clone();
                        move |menu, _, cx| {
                            let popup = cx.weak_entity();
                            let projection = entity.update(cx, |toolbar, _| {
                                toolbar.popup = Some(popup);
                                let units = toolbar.hidden_units();
                                let anchor = OverflowAnchor {
                                    bounds: toolbar.trigger_bounds,
                                    host: anchor.clone(),
                                };
                                toolbar.projection.build(&units, anchor).1
                            });
                            match projection {
                                Ok(items) => Self::popup(menu, items),
                                Err(_) => menu,
                            }
                        }
                    },
                )
                .with_anchor(&anchor),
            )
    }
}

fn inline_units(units: &[OverflowUnit], window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    units
        .iter()
        .map(|unit| {
            div()
                .id(gpui_kit::SharedString::from(format!(
                    "overflow-unit:{}",
                    unit.key
                )))
                .test_support()
                .flex_shrink_0()
                .child((unit.inline)(window, cx))
                .into_any_element()
        })
        .collect()
}

fn device_grid(value: Rems, window: &Window) -> f32 {
    let scale = window.scale_factor();
    (f32::from(value.to_pixels(window.rem_size())) * scale).round() / scale
}

impl Render for OverflowToolbar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let size = Metrics::current(cx);
        // Unit measurements and viewport bounds already use the device grid. Keep
        // spacing on that same grid so a fractional rem cannot create false overflow.
        self.gap = device_grid(size.gap(), window);
        self.chevron_width = device_grid(size.overflow_width(), window);
        self.measure_units(window, cx);
        let anchor = self
            .anchor
            .get_or_insert_with(|| cx.new(crate::anchored::Anchor::new))
            .clone();
        self.refresh_popup(&anchor, window, cx);
        let plan = self.plan();
        let mut children = inline_units(&self.units[..plan.visible], window, cx);
        if plan.has_overflow() {
            children.push(self.more_button(anchor, cx).into_any_element());
        } else {
            // The chevron renders the popup; once everything fits it is gone,
            // so a menu left open would stay open unseen.
            self.dismiss(window, cx);
        }
        let entity = cx.weak_entity();
        // The clip is on a frame a focus ring's width larger than the row on
        // every side, which its negative margin keeps out of the layout, so a
        // focused unit's ring shows. The row itself keeps the width it plans
        // with: its bounds are the units' allocation.
        let ring = px(FOCUS_RING_PX);
        let row = h_flex()
            .id("overflow-toolbar-row")
            .test_support()
            .flex_nowrap()
            .w(px(self.content_width()))
            .max_w_full()
            .min_w_0()
            .gap_x(px(self.gap))
            .on_prepaint(move |bounds, window, cx| {
                _ = entity.update(cx, |toolbar, cx| {
                    let width = f32::from(bounds.size.width);
                    let changed = toolbar
                        .viewport
                        .is_none_or(|viewport| crate::measure::changed(viewport, width));
                    if changed {
                        toolbar.viewport = Some(width);
                        toolbar.popup_dirty = true;
                        cx.notify();
                        // Native accessibility actions can finish on this first measured frame.
                        window.request_animation_frame();
                    }
                });
            })
            .children(children);
        h_flex()
            .id("overflow-toolbar-clip")
            .test_support()
            .min_w_0()
            .m(-ring)
            .p(ring)
            .overflow_hidden()
            .child(row)
    }
}
