//! Deferred composition lets the bar apply its controlled selection to each tab.

use gpui_kit::component::tab::{Tab as KitTab, TabBar as KitTabBar};
use gpui_kit::component::{ActiveTheme as _, Sizable, Size};
use gpui_kit::{
    App, ClickEvent, ElementId, InteractiveElement, Interactivity, IntoElement, RenderOnce,
    SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Window,
};

use super::{TabSpec, paint};

pub struct Tab {
    pub(super) native: KitTab,
    pub(super) spec: TabSpec,
    pub(super) close: Option<super::paint::CloseHandler>,
}

// qual:allow(dry, boilerplate) reason: "These methods forward native Tab callbacks and accessibility while retaining the controlled visual facts."
impl Tab {
    pub(super) fn new(spec: TabSpec) -> Self {
        Self {
            native: KitTab::new()
                .aria_label(spec.accessible_label())
                .disabled(spec.disabled),
            spec,
            close: None,
        }
    }

    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.native = self.native.aria_label(label);
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.native = self.native.on_click(handler);
        self
    }

    pub fn on_close(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.close = Some(std::rc::Rc::new(handler));
        self
    }
}

impl gpui_kit::prelude::FluentBuilder for Tab {}

impl InteractiveElement for Tab {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.native.interactivity()
    }
}
impl StatefulInteractiveElement for Tab {}

#[derive(IntoElement)]
pub struct TabBar {
    native: KitTabBar,
    tabs: Vec<Tab>,
    selected: Option<usize>,
    surface: Option<gpui_kit::Hsla>,
    size: Size,
    suffix: Option<gpui_kit::AnyElement>,
    has_last_empty_space: bool,
    pane: bool,
}

// qual:allow(dry, boilerplate) reason: "The fluent TabBar adapter defers native children until controlled selection is known; generated setters would bypass that contract."
impl TabBar {
    pub(super) fn new(id: impl Into<ElementId>) -> Self {
        Self {
            native: KitTabBar::new(id).small().w_auto(),
            tabs: Vec::new(),
            selected: None,
            surface: None,
            size: Size::Small,
            suffix: None,
            has_last_empty_space: false,
            pane: false,
        }
    }

    /// The bar of a workspace pane's tab strip, which names what is open:
    /// tabs as tall as the strip on the strip's own surface, the open one
    /// taking the content's surface over the bar's bottom hairline, with the
    /// native hairline on its trailing side and no accent line along its top.
    pub(super) fn pane(mut self) -> Self {
        self.pane = true;
        self.native = self.native.bg(gpui_kit::transparent_black());
        self
    }

    /// The adjoining content/header surface; defaults to the main content background.
    pub fn surface(mut self, surface: gpui_kit::Hsla) -> Self {
        self.surface = Some(surface);
        self
    }

    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected = Some(index);
        self.native = self.native.selected_index(index);
        self
    }

    pub fn child(mut self, tab: Tab) -> Self {
        self.tabs.push(tab);
        self
    }

    pub fn children(mut self, tabs: impl IntoIterator<Item = Tab>) -> Self {
        self.tabs.extend(tabs);
        self
    }

    pub fn suffix(mut self, suffix: impl IntoElement) -> Self {
        self.suffix = Some(suffix.into_any_element());
        self
    }

    pub fn prefix(mut self, prefix: impl IntoElement) -> Self {
        self.native = self.native.prefix(prefix);
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.native = self.native.on_click(handler);
        self
    }

    pub fn last_empty_space(mut self, space: impl IntoElement) -> Self {
        self.native = self.native.last_empty_space(space);
        self.has_last_empty_space = true;
        self
    }

    pub fn track_scroll(mut self, scroll: &gpui_kit::ScrollHandle) -> Self {
        self.native = self.native.track_scroll(scroll);
        self
    }
}

impl Sizable for TabBar {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self.native = self.native.with_size(self.size);
        self
    }
}

impl Styled for TabBar {
    fn style(&mut self) -> &mut StyleRefinement {
        self.native.style()
    }
}

impl RenderOnce for TabBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = crate::Tokens::current(cx);
        // Kit only renders last_empty_space when a suffix or menu exists.
        // Supply an empty suffix only if the caller has not supplied its own.
        let native = match self.suffix {
            Some(suffix) => self.native.suffix(suffix),
            None if self.has_last_empty_space => self.native.suffix(gpui_kit::Empty),
            None => self.native,
        };
        let dress = paint::Dress {
            surface: self.surface.unwrap_or(tokens.bg),
            size: self.size,
            pane: self.pane,
            mono: cx.theme().mono_font_family.clone(),
        };
        native.children(
            self.tabs.into_iter().enumerate().map(|(ix, tab)| {
                paint::tab(tab, self.selected == Some(ix), &tokens, dress.clone())
            }),
        )
    }
}
