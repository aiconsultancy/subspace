//! Icon-bearing application actions. One global relative scale, normal text,
//! native Button behavior, and one explicit compact preset for approved inline uses.

mod content;
mod forwarding;
mod quiet;
mod size;
#[cfg(test)]
mod tests;

pub use quiet::{Paint, glyph_latch, paint};
pub use size::{IconButtonSize, Metrics};

pub const PREFERENCE_KEY: &str = "app.icon-button-size";

/// How tall a button with `role` is at the current icon-button size. The
/// compact role is a fixed preset; the others follow the size.
pub(crate) fn role_target(role: ButtonRole, cx: &App) -> Rems {
    let tokens = Tokens::current(cx);
    let geometry = Geometry::resolve(
        role,
        tokens.controls.button(role),
        IconButtonSize::current(cx),
    );
    geometry.target
}

/// The compact role's glyph and gap, for a mark that sits beside compact
/// buttons, such as the status bar's chat engine.
pub fn compact_glyph(tokens: &Tokens) -> (Rems, Rems) {
    let geometry = Geometry::resolve(
        ButtonRole::Compact,
        tokens.controls.compact_button,
        IconButtonSize::Small,
    );
    (geometry.glyph, geometry.gap)
}

/// Restore the shared appearance preference after the product's Prefs are loaded.
pub fn initialize(cx: &mut App) {
    IconButtonSize::from_key(crate::prefs::get(cx, PREFERENCE_KEY).as_deref()).apply(cx);
}

use crate::Tokens;
use crate::tokens::{ButtonRole, Controls, ToolbarLook};
use content::Geometry;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Selectable as _, Sizable as _,
    button::{Button, ButtonRounded, ButtonVariant, ButtonVariants as _},
};
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Rems, RenderOnce, SharedString, Window,
};

/// A native button with independently sized glyph and target. Construct it
/// without an App; the effective global size is read when it renders.
#[derive(IntoElement)]
pub struct IconButton {
    button: Button,
    icon: Option<Icon>,
    label: Option<SharedString>,
    loading: bool,
    disabled: bool,
    focus_on_click: bool,
    role: ButtonRole,
    /// The native variant, outline and selection, mirrored because the native
    /// Button does not expose them and the Liquid Glass style depends on them.
    variant: ButtonVariant,
    outline: bool,
    selected: bool,
    /// Flat in every state whatever the theme's toolbar look.
    flat: bool,
    /// Latched, shown by the accent glyph alone; see [`Self::latched_glyph`].
    glyph_latch: bool,
    explicit_radius: bool,
    explicit_name: bool,
    children: Vec<AnyElement>,
    focus: Option<FocusHandle>,
}

// qual:allow(dry, boilerplate) reason: "The fluent API must retain native Button builder semantics while icon, text and loading overrides stay in one render-time presentation. A generated builder would not forward those contracts."
impl IconButton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self::from_button(Button::new(id).small()).ghost()
    }

    /// Retain the supplied native ID, callbacks and focus behavior, including
    /// a DialogClose trigger. Supply icon and label through this builder.
    /// The template must be unadorned, as Kit 0.9.1 DialogClose::trigger is:
    /// Kit exposes no API to clear a pre-existing native icon, label or children.
    /// Configure disabled and loading state through this wrapper as well.
    pub fn from_button(button: Button) -> Self {
        let selected = button.is_selected();
        Self {
            button,
            icon: None,
            label: None,
            loading: false,
            disabled: false,
            focus_on_click: false,
            role: ButtonRole::default(),
            variant: ButtonVariant::default(),
            outline: false,
            selected,
            flat: false,
            glyph_latch: false,
            explicit_radius: false,
            explicit_name: false,
            children: Vec::new(),
            focus: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Approved dense inline actions only: the theme's compact role, a fixed
    /// preset (20px target and 12px glyph at rem16 by default). This does not
    /// opt ordinary toolbar or menu triggers out of the global size.
    pub fn compact(self) -> Self {
        self.role(ButtonRole::Compact)
    }

    /// Take the theme's geometry for `role`. New buttons are toolbar buttons.
    pub fn role(mut self, role: ButtonRole) -> Self {
        self.role = role;
        self
    }

    /// A quiet button that is flat in every state and every theme, whatever
    /// the theme's toolbar look: no capsule, rim or glass, the `button-hover`
    /// fill under the pointer. The status bar's commands use it.
    pub fn flat(mut self) -> Self {
        self.flat = true;
        self.ghost()
    }

    /// A quieter [`Self::latched`] for a quiet glyph button: while on, the
    /// glyph takes the accent colour and nothing fills the button, so a
    /// switch that is usually on, such as a workspace dock switch, does not
    /// draw the eye. It keeps the accessible pressed state, and other latched
    /// buttons keep their fill.
    pub fn latched_glyph(mut self, latched: bool) -> Self {
        self.glyph_latch = true;
        self.latched(latched)
    }

    /// How this button's surface follows `controls`, the theme's control
    /// style.
    pub fn surface(&self, controls: &Controls) -> Surface {
        Style {
            glass: controls.liquid_glass,
            look: controls.toolbar_look,
            flat: self.flat,
            role: self.role,
            variant: self.variant,
            outline: self.outline,
            has_icon: self.icon.is_some(),
        }
        .surface()
    }

    /// Keep a feature's retained focus handle as the native button's real handle.
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    /// Retain focus after pointer activation, for disclosures that continue
    /// with Space or Enter. Ordinary toolbar buttons keep native focus behavior.
    pub fn focus_on_click(mut self) -> Self {
        self.focus_on_click = true;
        self
    }

    pub(crate) fn render_native(mut self, window: &mut Window, cx: &mut App) -> AnyElement {
        let focus = self.native_focus(window, cx);
        if self.focus_on_click && !self.disabled && !self.loading {
            // Append to Interactivity, not Button::on_click: a supplied native
            // template may already own its activation callback (DialogClose).
            self.button
                .interactivity()
                .on_click(move |event, window, cx| {
                    if matches!(event, ClickEvent::Mouse(_)) {
                        focus.focus(window, cx);
                    }
                });
        }
        let rem = window.rem_size();
        self.native(rem, cx).render(window, cx).into_any_element()
    }

    fn native_focus(&mut self, window: &mut Window, cx: &mut App) -> FocusHandle {
        let id = self
            .button
            .interactivity()
            .element_id
            .clone()
            .unwrap_or(0.into());
        let state = window.use_keyed_state(id, cx, |_, cx| cx.focus_handle());
        if let Some(focus) = &self.focus {
            state.update(cx, |state, _| *state = focus.clone());
        }
        state.read(cx).clone()
    }

    fn native(self, rem: Pixels, cx: &App) -> Button {
        let tokens = Tokens::current(cx);
        let finish = self.finish(&tokens, cx);
        let hover_glyph = match &finish {
            Finish::Quiet(paint) if !self.disabled => Some(paint.hover_glyph),
            _ => None,
        };
        let glyph = match &finish {
            Finish::Quiet(paint) => Some(paint.glyph),
            _ => None,
        };
        let mut button = self.button;
        if !self.explicit_name
            && let Some(label) = &self.label
        {
            button = button.accessibility_label(label.clone());
        }
        if self.icon.is_none() && self.label.is_none() {
            // A row of caller content, such as a member row, keeps native geometry.
            return button.loading(self.loading).children(self.children);
        }
        let metrics = tokens.controls.button(self.role);
        let geometry = Geometry::resolve(self.role, metrics, IconButtonSize::current(cx));
        if !self.explicit_radius {
            button = button.rounded(ButtonRounded::Size(geometry.radius_at(rem)));
        }
        content::compose(
            finish.dress(button, &tokens, cx),
            content::Content {
                icon: self.icon,
                label: self.label,
                loading: self.loading,
                children: self.children,
                hover_glyph,
                glyph,
            },
            geometry,
        )
    }

    /// The surface's paint: a quiet button's glyph starts from GPUI Kit's
    /// ghost colour.
    fn finish(&self, tokens: &Tokens, cx: &App) -> Finish {
        match self.surface(&tokens.controls) {
            Surface::Quiet(look) => Finish::Quiet(self.quiet_paint(look, tokens, cx)),
            Surface::Glass => Finish::Glass(self.selected),
            Surface::Tinted => Finish::Tinted,
            Surface::Native => Finish::Native,
        }
    }

    /// A quiet button's paint: a glyph latch shows its state by the glyph
    /// alone, any other latched button by its fill too.
    fn quiet_paint(&self, look: ToolbarLook, tokens: &Tokens, cx: &App) -> Paint {
        let glyph = cx.theme().secondary_foreground;
        if self.glyph_latch {
            glyph_latch(look, self.selected, tokens, glyph)
        } else {
            paint(look, self.selected, tokens, glyph)
        }
    }
}

/// A resolved [`Surface`] with what it needs to dress the native button.
enum Finish {
    Native,
    Quiet(Paint),
    /// Glass, lit while selected.
    Glass(bool),
    Tinted,
}

impl Finish {
    fn dress(self, button: Button, tokens: &Tokens, cx: &App) -> Button {
        match self {
            Self::Native => button,
            Self::Quiet(paint) => quiet::dress(button, &paint, tokens, cx),
            Self::Glass(selected) => crate::glass::control(button, selected, tokens, cx),
            Self::Tinted => button.outline(),
        }
    }
}

/// How a button's surface follows the theme's control style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// GPUI Kit's own variant.
    Native,
    /// A quiet glyph button in the finishes `look` chooses; see [`paint`].
    /// Toolbar buttons follow the theme; the status bar is always flat.
    Quiet(ToolbarLook),
    /// An outlined toolbar button, such as a menu picker, on a resting
    /// Liquid Glass capsule.
    Glass,
    /// A destructive action at secondary prominence: red text and edge, no fill.
    Tinted,
}

struct Style {
    glass: bool,
    look: ToolbarLook,
    flat: bool,
    role: ButtonRole,
    variant: ButtonVariant,
    outline: bool,
    has_icon: bool,
}

impl Style {
    fn surface(&self) -> Surface {
        let ghost = self.variant.is_ghost();
        let outlined = self.outline && (ghost || self.variant == ButtonVariant::Default);
        let toolbar_glyph = self.role == ButtonRole::Toolbar && self.has_icon;
        match (self.flat, self.glass, self.variant) {
            (true, _, _) => Surface::Quiet(ToolbarLook::FLAT),
            (false, true, ButtonVariant::Danger) => Surface::Tinted,
            _ if toolbar_glyph && ghost && !outlined => Surface::Quiet(self.look),
            _ if toolbar_glyph && outlined && self.glass => Surface::Glass,
            _ => Surface::Native,
        }
    }
}

impl RenderOnce for IconButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.render_native(window, cx)
    }
}
