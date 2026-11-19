//! One product-composed panel that the shell docks beside every Space, such
//! as the terminal. The panel owns its content and its header; the shell owns
//! whether it shows, where it docks, its size and whether it fills the
//! workspace, and tells the panel through [`PanelLayout`]. The panel asks the
//! shell for changes with [`DockPanelEvent`]s.

use gpui_kit::{
    AnyView, App, AppContext as _, Entity, EventEmitter, FocusHandle, Focusable, Pixels, Render,
    SharedString, Size, Subscription, Window,
};

use crate::settings::SettingsEditorFactory;
use crate::{HelpImage, KeyHelp};

/// The window edge the panel docks against.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Placement {
    Left,
    #[default]
    Bottom,
    Right,
}

impl Placement {
    pub const ALL: [Self; 3] = [Self::Left, Self::Bottom, Self::Right];

    /// The stored value.
    pub fn key(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Bottom => "bottom",
            Self::Right => "right",
        }
    }

    /// The stored value read back; anything else is `None`.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|placement| placement.key() == value)
    }

    /// The name a menu or a select shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Bottom => "Bottom",
            Self::Right => "Right",
        }
    }
}

/// How wide a bottom panel is: beside the chat, which keeps the full body
/// height, or across the whole window below the Space and the chat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BottomSpan {
    #[default]
    BesideChat,
    FullWidth,
}

impl BottomSpan {
    pub const ALL: [Self; 2] = [Self::BesideChat, Self::FullWidth];

    /// The stored value.
    pub fn key(self) -> &'static str {
        match self {
            Self::BesideChat => "beside-chat",
            Self::FullWidth => "full-width",
        }
    }

    /// The stored value read back; anything else is `None`.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|span| span.key() == value)
    }

    /// The name a select shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::BesideChat => "Beside the chat",
            Self::FullWidth => "Full window width",
        }
    }
}

/// What the shell shows, for the panel's latched header buttons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PanelLayout {
    pub placement: Placement,
    pub span: BottomSpan,
    pub maximized: bool,
    /// Whether the header's layout buttons show their text.
    pub labels: bool,
}

/// What a dock panel asks of the shell. The shell applies each request at
/// once, stores it and sends the panel the new [`PanelLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPanelEvent {
    Hide,
    ToggleMaximized,
    Move(Placement),
    SetBottomSpan(BottomSpan),
    /// Show or hide the text of the header's layout buttons.
    SetLabels(bool),
}

/// A panel the shell docks. Created once per window by its registration's
/// `create`; it keeps running while hidden.
pub trait DockPanel: Render + Focusable + EventEmitter<DockPanelEvent> {
    /// The panel is shown: start its content when this is the first time and
    /// focus it.
    fn show(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>);

    /// The shell's current layout, for the header's latched buttons.
    fn set_layout(&mut self, layout: PanelLayout, cx: &mut gpui_kit::Context<Self>);

    /// Running work that quitting would end, named for the quit confirmation.
    fn quit_warnings(&self, _cx: &App) -> Vec<SharedString> {
        Vec::new()
    }

    /// A short status the shell adds to the status bar, such as configuration
    /// warnings.
    fn status(&self, _cx: &App) -> Option<SharedString> {
        None
    }

    /// The least size the panel's content needs. The shell keeps the Space
    /// at its own minimum beside it.
    fn minimum_size(&self, _cx: &App) -> Option<Size<Pixels>> {
        None
    }

    /// Reload the panel's configuration, as Reload does while it has focus.
    fn reload(&mut self, _window: &mut Window, _cx: &mut gpui_kit::Context<Self>) {}
}

/// Receives a dock panel's events in the shell.
pub type DockEventSink = Box<dyn Fn(&DockPanelEvent, &mut Window, &mut App)>;

trait AnyDockPanel {
    fn view(&self) -> AnyView;
    fn focus_handle(&self, cx: &App) -> FocusHandle;
    fn show(&self, window: &mut Window, cx: &mut App);
    fn set_layout(&self, layout: PanelLayout, cx: &mut App);
    fn quit_warnings(&self, cx: &App) -> Vec<SharedString>;
    fn status(&self, cx: &App) -> Option<SharedString>;
    fn minimum_size(&self, cx: &App) -> Option<Size<Pixels>>;
    fn reload(&self, window: &mut Window, cx: &mut App);
    fn subscribe(&self, window: &mut Window, cx: &mut App, sink: DockEventSink) -> Subscription;
}

impl<P: DockPanel> AnyDockPanel for Entity<P> {
    fn view(&self) -> AnyView {
        self.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn show(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |panel, cx| panel.show(window, cx));
    }

    fn set_layout(&self, layout: PanelLayout, cx: &mut App) {
        self.update(cx, |panel, cx| panel.set_layout(layout, cx));
    }

    fn quit_warnings(&self, cx: &App) -> Vec<SharedString> {
        self.read(cx).quit_warnings(cx)
    }

    fn status(&self, cx: &App) -> Option<SharedString> {
        self.read(cx).status(cx)
    }

    fn minimum_size(&self, cx: &App) -> Option<Size<Pixels>> {
        self.read(cx).minimum_size(cx)
    }

    fn reload(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |panel, cx| panel.reload(window, cx));
    }

    fn subscribe(&self, window: &mut Window, cx: &mut App, sink: DockEventSink) -> Subscription {
        window.subscribe(self, cx, move |_, event: &DockPanelEvent, window, cx| {
            sink(event, window, cx);
        })
    }
}

/// A created dock panel, with its type erased for the shell.
pub struct DockPanelHandle {
    inner: Box<dyn AnyDockPanel>,
}

impl DockPanelHandle {
    pub fn new<P: DockPanel>(panel: Entity<P>) -> Self {
        Self {
            inner: Box::new(panel),
        }
    }

    /// Creates `P` with `create` and wraps it.
    pub fn create<P: DockPanel>(
        cx: &mut App,
        create: impl FnOnce(&mut gpui_kit::Context<P>) -> P,
    ) -> Self {
        Self::new(cx.new(create))
    }

    pub fn view(&self) -> AnyView {
        self.inner.view()
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.inner.focus_handle(cx)
    }

    /// See [`DockPanel::show`].
    pub fn show(&self, window: &mut Window, cx: &mut App) {
        self.inner.show(window, cx);
    }

    pub fn set_layout(&self, layout: PanelLayout, cx: &mut App) {
        self.inner.set_layout(layout, cx);
    }

    pub fn quit_warnings(&self, cx: &App) -> Vec<SharedString> {
        self.inner.quit_warnings(cx)
    }

    pub fn status(&self, cx: &App) -> Option<SharedString> {
        self.inner.status(cx)
    }

    pub fn minimum_size(&self, cx: &App) -> Option<Size<Pixels>> {
        self.inner.minimum_size(cx)
    }

    pub fn reload(&self, window: &mut Window, cx: &mut App) {
        self.inner.reload(window, cx);
    }

    pub fn subscribe(
        &self,
        window: &mut Window,
        cx: &mut App,
        sink: DockEventSink,
    ) -> Subscription {
        self.inner.subscribe(window, cx, sink)
    }
}

/// What a product supplies so the shell can dock its panel: how to create
/// it, its keys, its help guide and its settings editor. The editor edits the
/// config file's `terminal` object.
#[derive(Clone, Copy)]
pub struct DockPanelRegistration {
    pub create: fn(&mut Window, &mut App) -> DockPanelHandle,
    /// Binds the panel's own keys, once.
    pub bind_keys: fn(&mut App),
    pub help_markdown: &'static str,
    pub help_images: &'static [HelpImage],
    pub keys: &'static [KeyHelp],
    pub settings_editor: Option<SettingsEditorFactory>,
}

#[cfg(test)]
mod tests {
    use super::{BottomSpan, Placement};

    #[test]
    fn placements_and_spans_round_trip_their_stored_values() {
        for placement in Placement::ALL {
            assert_eq!(Placement::parse(placement.key()), Some(placement));
        }
        for span in BottomSpan::ALL {
            assert_eq!(BottomSpan::parse(span.key()), Some(span));
        }
        assert_eq!(Placement::parse("log-tab"), None);
        assert_eq!(Placement::default(), Placement::Bottom);
        assert_eq!(BottomSpan::default(), BottomSpan::BesideChat);
    }
}
