//! Live rail resize coordination and persisted resize outcomes.

use gpui_kit::base::{PANEL_MIN_SIZE, ResizableState};
use gpui_kit::{App, Context, Entity, Window, px};

use super::{NavigationMode, effective_collapsed_rail_width, save_collapsed, save_width, sizing};
use crate::app::SubspaceApp;

const WIDTH_EPS: f32 = 0.5;

impl SubspaceApp {
    pub(in crate::app) fn live_rail_width(&self, cx: &App) -> Option<f32> {
        self.rail_state
            .read(cx)
            .sizes()
            .first()
            .copied()
            .map(f32::from)
    }

    pub(in crate::app) fn rail_shows_labels(&self, cx: &App) -> bool {
        match self.live_rail_width(cx) {
            Some(width) => width > effective_collapsed_rail_width(cx),
            None => !self.navigation_collapsed,
        }
    }

    pub(in crate::app) fn finish_rail_resize(
        &mut self,
        state: &Entity<ResizableState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(&size) = state.read(cx).sizes().first() else {
            return;
        };
        let cap = self.rail_content_max(window, cx);
        let outcome = sizing::drag_to(f32::from(size), effective_collapsed_rail_width(cx), cap);
        self.navigation_collapsed = outcome.collapsed;
        save_collapsed(cx, outcome.collapsed);
        if let Some(preferred) = outcome.preferred {
            self.preferred_rail_width = Some(preferred);
            save_width(cx, preferred);
        }
        self.rail_synced_cap = Some(cap.max(effective_collapsed_rail_width(cx)));
        self.rail_synced_target = Some(f32::from(size));
        self.rail_laid_out = true;
        cx.notify();
    }

    pub(in crate::app) fn reconcile_rail(
        &mut self,
        cap: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.navigation_mode != NavigationMode::Vertical {
            return;
        }
        let min = effective_collapsed_rail_width(cx);
        let cap = cap.max(min);
        let target = sizing::displayed_width(
            self.navigation_collapsed,
            self.preferred_rail_width,
            cap,
            min,
        );
        let Some(live) = self.live_rail_width(cx) else {
            return;
        };
        let changed = self.rail_synced_cap != Some(cap) || self.rail_synced_target != Some(target);
        if changed {
            if !widths_match(live, target) {
                self.resize_rail(target, window, cx);
            }
            self.rail_synced_cap = Some(cap);
            self.rail_synced_target = Some(target);
            self.rail_laid_out = widths_match(live, target);
            return;
        }
        if self.rail_laid_out {
            return;
        }
        let placeholder = f32::from(PANEL_MIN_SIZE);
        if widths_match(live, placeholder) && !widths_match(target, live) {
            self.resize_rail(target, window, cx);
        } else if widths_match(live, target) {
            self.rail_laid_out = true;
        }
    }

    pub(in crate::app) fn sync_rail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rail_synced_cap = None;
        self.rail_synced_target = None;
        self.reconcile_rail(self.rail_content_max(window, cx), window, cx);
    }

    pub(in crate::app) fn queue_rail_restore(
        &mut self,
        cap: f32,
        target: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live_rail_width(cx) else {
            return;
        };
        let cap = cap.max(effective_collapsed_rail_width(cx));
        let changed = self.rail_synced_cap != Some(cap) || self.rail_synced_target != Some(target);
        let placeholder = f32::from(PANEL_MIN_SIZE);
        let stuck =
            !self.rail_laid_out && widths_match(live, placeholder) && !widths_match(target, live);
        if !changed && !stuck {
            if widths_match(live, target) {
                self.rail_laid_out = true;
            }
            return;
        }
        if widths_match(live, target) {
            self.rail_synced_cap = Some(cap);
            self.rail_synced_target = Some(target);
            self.rail_laid_out = true;
            return;
        }
        let state = self.rail_state.clone();
        window.defer(cx, move |window, cx| {
            state.update(cx, |state, cx| {
                state.resize_panel(0, px(target), window, cx);
            });
        });
        self.rail_synced_cap = Some(cap);
        self.rail_synced_target = Some(target);
    }

    fn resize_rail(&mut self, target: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.rail_state.update(cx, |state, cx| {
            state.resize_panel(0, px(target), window, cx);
        });
    }
}

pub(super) fn widths_match(left: f32, right: f32) -> bool {
    (left - right).abs() <= WIDTH_EPS
}
