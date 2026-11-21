//! Canonical dashboard treatment for native GPUI Kit tabs.

use gpui_kit::component::Icon;
use gpui_kit::{AnyElement, ElementId, Hsla, SharedString};

/// Between a tab's glyph, title and status, as between a Button's icon and
/// label.
pub(super) const CONTENT_GAP: f32 = 6.;

/// Optional visible and accessible state shown after a tab label.
#[derive(Debug, Clone, PartialEq)]
pub struct TabStatus {
    visible: SharedString,
    accessible: SharedString,
    colour: Hsla,
}

impl TabStatus {
    pub fn text(
        visible: impl Into<SharedString>,
        accessible: impl Into<SharedString>,
        colour: Hsla,
    ) -> Self {
        Self {
            visible: visible.into(),
            accessible: accessible.into(),
            colour,
        }
    }

    pub fn count(count: u64, colour: Hsla) -> Self {
        let count = count.to_string();
        Self {
            visible: count.clone().into(),
            accessible: format!("{count} items").into(),
            colour,
        }
    }

    pub fn busy(colour: Hsla) -> Self {
        Self {
            visible: "…".into(),
            accessible: "Loading".into(),
            colour,
        }
    }

    pub fn count_and_busy(count: u64, colour: Hsla) -> Self {
        Self {
            visible: format!("{count} …").into(),
            accessible: format!("{count} items, loading").into(),
            colour,
        }
    }
}

/// Board-owned tab facts. An icon is the default; accessibility defaults to
/// the visible label plus any status text.
pub struct TabSpec {
    label: SharedString,
    /// An opened item's key, such as `SUB-12`, shown before the label.
    key: Option<SharedString>,
    icon: Option<Icon>,
    status: Option<TabStatus>,
    disabled: bool,
    unread: bool,
    content: Option<AnyElement>,
}

// qual:allow(dry, boilerplate) reason: "Controlled tab facts include a native icon and composed dock content; a derive builder cannot preserve the default constructors and accessible status contract."
impl TabSpec {
    pub fn new(label: impl Into<SharedString>, icon: impl Into<Icon>) -> Self {
        Self {
            label: label.into(),
            key: None,
            icon: Some(icon.into()),
            status: None,
            disabled: false,
            unread: false,
            content: None,
        }
    }

    /// A text-only tab for existing named regions whose titles need no glyph.
    pub fn plain(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            key: None,
            icon: None,
            status: None,
            disabled: false,
            unread: false,
            content: None,
        }
    }

    /// An opened item's key, such as `SUB-12` or `#42`, shown after the
    /// glyph in the monospace family and the muted colour, before the
    /// label, which is then the item's title. The accessible name starts
    /// with it.
    pub fn key(mut self, key: impl Into<SharedString>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Unread state is independent of selection and optional status text.
    pub fn unread(mut self, unread: bool) -> Self {
        self.unread = unread;
        self
    }

    /// A dock panel's composed native title, with a separate accessible name.
    pub fn content(mut self, content: AnyElement) -> Self {
        self.content = Some(content);
        self
    }

    pub fn status(mut self, status: TabStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    fn accessible_label(&self) -> SharedString {
        let name: SharedString = match &self.key {
            Some(key) => format!("{key} {}", self.label).into(),
            None => self.label.clone(),
        };
        let label = self.status.as_ref().map_or_else(
            || name.clone(),
            |status| format!("{name}, {}", status.accessible).into(),
        );
        if self.unread {
            format!("{label}, unread items").into()
        } else {
            label
        }
    }
}

impl gpui_kit::prelude::FluentBuilder for TabSpec {}

mod bar;
pub mod dock;
#[cfg(test)]
mod interaction_tests;
mod paint;
#[cfg(test)]
mod state_tests;

pub use bar::{Tab, TabBar};
pub use paint::unread_dot;

/// Build a native tab with the shared surface treatment.
pub fn tab(spec: TabSpec) -> Tab {
    Tab::new(spec)
}

/// Build a native section/resource tab bar at its natural width.
pub fn tab_bar(id: impl Into<ElementId>) -> TabBar {
    TabBar::new(id)
}

/// Build the tab bar of a workspace pane's tab strip
/// (`workspace::Pane::tabs`), which names what is open. Each tab is as tall
/// as the strip, on the strip's surface. The open tab takes the content's
/// surface over the strip's bottom hairline and the native hairline on its
/// trailing side, with no accent line along its top, so it reads as part of
/// the content below.
pub fn pane_tab_bar(id: impl Into<ElementId>) -> TabBar {
    TabBar::new(id).pane()
}

#[cfg(test)]
mod tests {
    use gpui_kit::hsla;
    use subspace_lucide::IconName;

    use super::{TabSpec, TabStatus};

    #[test]
    fn count_and_busy_statuses_keep_visible_and_accessible_meaning() {
        let count = TabStatus::count(12, hsla(0., 0., 0., 1.));
        assert_eq!(count.visible, "12");
        assert_eq!(count.accessible, "12 items");

        let busy = TabStatus::busy(hsla(0., 0., 0., 1.));
        assert_eq!(busy.visible, "…");
        assert_eq!(busy.accessible, "Loading");

        let both = TabStatus::count_and_busy(12, hsla(0., 0., 0., 1.));
        assert_eq!(both.visible, "12 …");
        assert_eq!(both.accessible, "12 items, loading");

        let disabled = TabSpec::new("Open", IconName::CircleDot).disabled(true);
        assert!(disabled.disabled);
        assert_eq!(disabled.accessible_label(), "Open");

        let item = TabSpec::new("Course correction", IconName::CircleDot).key("SUB-12");
        assert_eq!(item.accessible_label(), "SUB-12 Course correction");
    }
}
