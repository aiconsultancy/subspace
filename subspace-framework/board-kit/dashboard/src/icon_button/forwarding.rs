use super::IconButton;
use gpui_kit::component::{
    Disableable, FocusableExt, Selectable, Sizable, Size,
    button::{ButtonRounded, ButtonVariant, ButtonVariants},
};
use gpui_kit::{
    AnyElement, App, ClickEvent, InteractiveElement, Interactivity, ParentElement, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, Window,
};

// qual:allow(dry, boilerplate) reason: "These methods forward the existing native Button contract; adding a generated builder or a dependency would not replace the required behavior adapters."
impl IconButton {
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.button = self.button.accessibility_label(label);
        self.explicit_name = true;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.button = self.button.tooltip(tooltip);
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.button = self.button.on_click(handler);
        self
    }

    pub fn rounded(mut self, rounded: impl Into<ButtonRounded>) -> Self {
        self.button = self.button.rounded(rounded);
        self.explicit_radius = true;
        self
    }

    pub fn outline(mut self) -> Self {
        self.button = self.button.outline();
        self.outline = true;
        self
    }

    pub fn toggled(mut self, value: bool) -> Self {
        self.button = self.button.toggled(value);
        self
    }

    /// Show an owner-controlled toolbar toggle using native selected styling.
    /// Both values expose the native Button's accessible pressed state.
    /// The caller updates its value from `on_click`; this never changes it.
    pub fn latched(mut self, latched: bool) -> Self {
        self.button = self.button.selected(latched).toggled(latched);
        self.selected = latched;
        self
    }

    pub fn dropdown_caret(mut self, value: bool) -> Self {
        self.button = self.button.dropdown_caret(value);
        self
    }
}

impl Styled for IconButton {
    fn style(&mut self) -> &mut StyleRefinement {
        self.button.style()
    }
}

impl InteractiveElement for IconButton {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.button.interactivity()
    }
}

impl StatefulInteractiveElement for IconButton {}

impl ParentElement for IconButton {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Disableable for IconButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.button = self.button.disabled(disabled);
        self.disabled = disabled;
        self
    }
}

impl Selectable for IconButton {
    fn selected(mut self, selected: bool) -> Self {
        self.button = self.button.selected(selected);
        self.selected = selected;
        self
    }
    fn is_selected(&self) -> bool {
        self.button.is_selected()
    }
    fn open(mut self, open: bool) -> Self {
        self.button = self.button.open(open);
        self
    }
    fn is_open(&self) -> bool {
        self.button.is_open()
    }
}

impl Sizable for IconButton {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.button = self.button.with_size(size);
        self
    }
}

impl FocusableExt for IconButton {
    fn focus_ring(mut self, enabled: bool) -> Self {
        self.button = self.button.focus_ring(enabled);
        self
    }
    fn is_focus_ring_enabled(&self) -> bool {
        self.button.is_focus_ring_enabled()
    }
}

impl ButtonVariants for IconButton {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.button = self.button.with_variant(variant);
        self.variant = variant;
        self
    }
}
