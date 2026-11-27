//! A measured, source-ordered toolbar row with a native overflow menu.

mod projection;
#[cfg(test)]
mod runtime_tests;
mod view;

use std::rc::Rc;

use crate::action::Intent;
use crate::anchored::Anchor;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, IntoElement, Pixels, SharedString,
    WeakEntity, Window,
};
use subspace_lucide::IconName;

const DEFAULT_GAP: f32 = 4.;
const DEFAULT_CHEVRON_WIDTH: f32 = 28.;
/// A measured row this wide, or narrower, has room for no unit.
const NO_WIDTH: f32 = 0.;

/// Builds presentation only. Called for layout measurement as well as visible rendering.
pub type InlineBuilder = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

#[derive(Clone)]
pub struct OverflowAnchor {
    bounds: Bounds<Pixels>,
    host: Entity<Anchor>,
}

impl OverflowAnchor {
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }
    pub fn host(&self) -> Entity<Anchor> {
        self.host.clone()
    }
}

/// A popup item is built afresh so native GPUI menu state, callbacks and
/// shortcut hints remain intact when the menu is reopened.
/// Keep icons fixed for each unit key and local entry index: pinned native
/// Icon has no public equality or source reader. Use a new key if an icon changes.
#[derive(Clone)]
pub struct PopupEntry(Rc<dyn Fn(OverflowAnchor) -> PopupMenuItem>);

impl PopupEntry {
    pub fn new(builder: impl Fn() -> PopupMenuItem + 'static) -> Self {
        Self(Rc::new(move |_| builder()))
    }

    fn build(&self, anchor: OverflowAnchor) -> PopupMenuItem {
        (self.0)(anchor)
    }

    pub fn anchored(builder: impl Fn(OverflowAnchor) -> PopupMenuItem + 'static) -> Self {
        Self(Rc::new(builder))
    }
}

/// One atomic inline unit. Segmented controls use one unit with multiple
/// [`PopupEntry`] values; arbitrary `AnyElement` values cannot be projected.
#[derive(Clone)]
pub struct OverflowUnit {
    pub key: SharedString,
    pub measured_width: f32,
    pub inline: InlineBuilder,
    pub popup_entries: Vec<PopupEntry>,
}

impl OverflowUnit {
    pub fn new(
        key: impl Into<SharedString>,
        measured_width: f32,
        inline: InlineBuilder,
        popup_entries: Vec<PopupEntry>,
    ) -> Self {
        Self {
            key: key.into(),
            measured_width,
            inline,
            popup_entries,
        }
    }

    pub fn inline<F, E>(
        key: impl Into<SharedString>,
        measured_width: f32,
        inline: F,
        popup_entries: Vec<PopupEntry>,
    ) -> Self
    where
        F: Fn(&mut Window, &mut App) -> E + 'static,
        E: IntoElement,
    {
        Self::new(
            key,
            measured_width,
            Rc::new(move |window, cx| inline(window, cx).into_any_element()),
            popup_entries,
        )
    }

    /// A Space's Refresh, at the end of its toolbar: a quiet glyph button
    /// named `id` while it fits, else a Refresh entry in the overflow menu.
    /// Both run `reload`.
    pub fn refresh(id: &'static str, reload: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        let reload: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new(reload);
        let menu_reload = reload.clone();
        let entry = PopupEntry::new(move || {
            let reload = menu_reload.clone();
            PopupMenuItem::new(REFRESH)
                .icon(IconName::RefreshCw)
                .on_click(move |_, window, cx| reload(window, cx))
        });
        Self::inline(
            id,
            UNMEASURED_WIDTH,
            move |_, _| {
                let reload = reload.clone();
                crate::action::icon_button(id, Intent::Quiet, IconName::RefreshCw, REFRESH)
                    .on_click(move |_, window, cx| reload(window, cx))
            },
            vec![entry],
        )
    }
}

/// Refresh's label, on its button and in the overflow menu.
const REFRESH: &str = "Refresh";
/// A unit is measured when it first draws.
const UNMEASURED_WIDTH: f32 = 0.;

#[derive(Debug, Clone, PartialEq)]
pub struct OverflowPlan {
    pub visible: usize,
    pub hidden: usize,
}

impl OverflowPlan {
    pub fn has_overflow(&self) -> bool {
        self.hidden != 0
    }
}

/// Selects a visible prefix. Width zero means the first measurement has not
/// arrived; keeping all units in the row lets GPUI establish their real size.
pub fn plan_overflow(available: f32, widths: &[f32], gap: f32, chevron: f32) -> OverflowPlan {
    if available <= 0.0 || widths.iter().any(|width| *width <= 0.0) {
        return OverflowPlan {
            visible: widths.len(),
            hidden: 0,
        };
    }

    let total = widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    if total <= available {
        return OverflowPlan {
            visible: widths.len(),
            hidden: 0,
        };
    }

    let mut used = 0.0;
    let mut visible = 0;
    for width in widths {
        let next = used + *width + if visible > 0 { gap } else { 0.0 };
        if next + gap + chevron > available {
            break;
        }
        used = next;
        visible += 1;
    }
    OverflowPlan {
        visible,
        hidden: widths.len().saturating_sub(visible),
    }
}

/// State-bearing shared row. Consumers create it with `cx.new` and render the
/// resulting entity. Every unit is measured at its natural width; only the visible
/// prefix is painted. The row can shrink to its parent's flex allocation.
pub struct OverflowToolbar {
    units: Vec<OverflowUnit>,
    widths: Vec<f32>,
    /// The row's measured width, or `None` before its first measurement.
    viewport: Option<f32>,
    gap: f32,
    chevron_width: f32,
    trigger_bounds: Bounds<Pixels>,
    popup: Option<WeakEntity<PopupMenu>>,
    popup_dirty: bool,
    anchor: Option<Entity<Anchor>>,
    projection: projection::Projection,
}

impl OverflowToolbar {
    /// Initialize a consumer's row once, then refresh its units without losing measurements.
    pub fn sync(
        slot: &mut Option<Entity<Self>>,
        units: Vec<OverflowUnit>,
        cx: &mut App,
    ) -> Entity<Self> {
        let toolbar = slot.get_or_insert_with(|| cx.new(|_| Self::new(Vec::new())));
        toolbar.update(cx, |toolbar, cx| toolbar.set_units(units, cx));
        toolbar.clone()
    }

    pub fn new(units: Vec<OverflowUnit>) -> Self {
        let widths = units.iter().map(|unit| unit.measured_width).collect();
        Self {
            units,
            widths,
            viewport: None,
            gap: DEFAULT_GAP,
            chevron_width: DEFAULT_CHEVRON_WIDTH,
            trigger_bounds: Bounds::default(),
            popup: None,
            popup_dirty: false,
            anchor: None,
            projection: Default::default(),
        }
    }
    fn popup(menu: PopupMenu, items: Vec<PopupMenuItem>) -> PopupMenu {
        items.into_iter().fold(menu, |menu, item| menu.item(item))
    }

    /// Natural width of all units, excluding the overflow trigger. Before the
    /// first render this uses the supplied width estimates.
    pub fn content_width(&self) -> f32 {
        self.widths.iter().sum::<f32>() + self.gap * self.widths.len().saturating_sub(1) as f32
    }

    /// Replace the semantic units when their source state changes. Existing keyed
    /// measurements survive until the next render measures the new presentations.
    pub fn set_units(&mut self, units: Vec<OverflowUnit>, cx: &mut Context<Self>) {
        let old = self
            .units
            .iter()
            .zip(self.widths.iter())
            .map(|(unit, width)| (unit.key.clone(), *width));
        let old: std::collections::HashMap<_, _> = old.collect();
        self.units = units;
        self.widths = self
            .units
            .iter()
            .map(|unit| old.get(&unit.key).copied().unwrap_or(unit.measured_width))
            .collect();
        self.popup_dirty = true;
        cx.notify();
    }

    /// Close the retained chevron host when its owner is hidden or deactivated.
    pub fn dismiss(&self, window: &mut Window, cx: &mut App) {
        if let Some(anchor) = &self.anchor {
            anchor.update(cx, |anchor, cx| anchor.dismiss(window, cx));
        }
    }

    /// Which units show. Before the first measurement every unit does, so
    /// GPUI can size them. Once measured, a row given no width shows none:
    /// every unit is in the menu, not drawn where it cannot be seen.
    fn plan(&self) -> OverflowPlan {
        match self.viewport {
            None => OverflowPlan {
                visible: self.widths.len(),
                hidden: 0,
            },
            Some(width) if width <= NO_WIDTH => OverflowPlan {
                visible: 0,
                hidden: self.widths.len(),
            },
            Some(width) => plan_overflow(width, &self.widths, self.gap, self.chevron_width),
        }
    }

    pub(crate) fn anchor_for(&self, key: &str) -> Option<Entity<Anchor>> {
        let plan = self.plan();
        self.units
            .iter()
            .position(|unit| unit.key.as_ref() == key)
            .filter(|index| *index >= plan.visible)
            .and(self.anchor.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::plan_overflow;

    #[test]
    fn exact_fit_has_no_trigger() {
        let plan = plan_overflow(108., &[40., 60.], 8., 28.);
        assert_eq!((plan.visible, plan.hidden), (2, 0));
        assert_eq!(plan_overflow(107., &[40., 60.], 8., 28.).visible, 1);
    }

    #[test]
    fn chevron_reservation_prevents_cutoff_overlap() {
        let plan = plan_overflow(70., &[30., 30., 30.], 4., 28.);
        assert_eq!((plan.visible, plan.hidden), (1, 2));
    }

    #[test]
    fn gap_before_chevron_is_reserved() {
        let plan = plan_overflow(60., &[32., 32.], 8., 28.);
        assert_eq!((plan.visible, plan.hidden), (0, 2));
    }

    #[test]
    fn atomic_units_are_not_split() {
        let plan = plan_overflow(110., &[70., 40.], 8., 28.);
        assert_eq!((plan.visible, plan.hidden), (1, 1));
    }

    #[test]
    fn unmeasured_units_wait_for_real_measurement() {
        let plan = plan_overflow(40., &[0., 30.], 8., 28.);
        assert_eq!((plan.visible, plan.hidden), (2, 0));
    }

    #[test]
    fn growth_and_shrink_have_stable_prefixes() {
        assert_eq!(plan_overflow(90., &[30., 30., 30.], 4., 20.).visible, 2);
        assert_eq!(plan_overflow(110., &[30., 30., 30.], 4., 20.).visible, 3);
    }
}
