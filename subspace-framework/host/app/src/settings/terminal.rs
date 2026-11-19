//! The docked terminal's Settings drafts: its Layout choices, stored in the
//! state file, and its font editor, which edits the config file's
//! `terminal` object. Both apply on Save. Present only for a product with a
//! terminal.

use gpui_kit::{App, Context, Subscription, Window};
use serde_json::{Map, Value};
use subspace_core::settings::SettingsFile;
use subspace_dashboard::dock_panel::{BottomSpan, PanelLayout, Placement};
use subspace_dashboard::settings::SettingsEditor;

use super::Settings;
use super::form::{Choice, choice};

pub(super) struct TerminalDraft {
    pub(super) placement: Choice,
    pub(super) span: Choice,
    pub(super) labels: bool,
    initial: PanelLayout,
    pub(super) fonts: Option<FontDraft>,
}

pub(super) struct FontDraft {
    pub(super) editor: SettingsEditor,
    initial: Value,
    _subscription: Subscription,
}

/// The values a draft compares, for unsaved-change tracking.
#[derive(Clone, PartialEq)]
pub(super) struct TerminalSnapshot {
    placement: Option<String>,
    span: Option<String>,
    labels: bool,
    fonts: Option<Value>,
}

impl TerminalDraft {
    pub(super) fn load(
        file: &SettingsFile,
        window: &mut Window,
        cx: &mut Context<Settings>,
    ) -> Option<Self> {
        let registration = crate::terminal::registration(cx)?;
        let (_, initial) = crate::terminal::stored(cx);
        let fonts = registration
            .settings_editor
            .map(|factory| FontDraft::new(factory, file.values().terminal.clone(), window, cx));
        Some(Self {
            placement: choice(
                Placement::ALL
                    .map(|placement| placement.label().to_owned())
                    .to_vec(),
                Some(initial.placement.label()),
                false,
                window,
                cx,
            ),
            span: choice(
                BottomSpan::ALL.map(|span| span.label().to_owned()).to_vec(),
                Some(initial.span.label()),
                false,
                window,
                cx,
            ),
            labels: initial.labels,
            initial,
            fonts,
        })
    }

    /// The drafted layout. Maximize is not a stored choice.
    fn layout(&self, cx: &App) -> PanelLayout {
        let selected = |choice: &Choice| choice.read(cx).selected_value().cloned();
        PanelLayout {
            placement: placement_from_label(selected(&self.placement).as_deref()),
            span: span_from_label(selected(&self.span).as_deref()),
            maximized: false,
            labels: self.labels,
        }
    }

    /// Stores a changed layout; the shell applies it after Save.
    pub(super) fn save(&mut self, cx: &mut App) {
        let layout = self.layout(cx);
        if layout != self.initial {
            crate::terminal::store_layout(layout, cx);
            self.initial = layout;
        }
    }

    pub(super) fn snapshot(&self, cx: &App) -> TerminalSnapshot {
        let selected = |choice: &Choice| choice.read(cx).selected_value().cloned();
        TerminalSnapshot {
            placement: selected(&self.placement),
            span: selected(&self.span),
            labels: self.labels,
            fonts: self.fonts.as_ref().map(|fonts| fonts.editor.snapshot(cx)),
        }
    }

    /// Whether the font editor changed since opening or the last Save.
    pub(super) fn fonts_changed(&self, cx: &App) -> bool {
        self.fonts
            .as_ref()
            .is_some_and(|fonts| fonts.initial != fonts.editor.snapshot(cx))
    }

    /// The font editor's values when they changed, or why they cannot be
    /// saved.
    pub(super) fn font_edits(&self, cx: &App) -> Result<Option<Map<String, Value>>, String> {
        self.fonts
            .as_ref()
            .filter(|_| self.fonts_changed(cx))
            .map(|fonts| fonts.editor.values(cx))
            .transpose()
    }

    pub(super) fn accept_fonts(&mut self, cx: &App) {
        if let Some(fonts) = &mut self.fonts {
            fonts.initial = fonts.editor.snapshot(cx);
        }
    }
}

impl FontDraft {
    fn new(
        factory: subspace_dashboard::settings::SettingsEditorFactory,
        saved: Map<String, Value>,
        window: &mut Window,
        cx: &mut Context<Settings>,
    ) -> Self {
        let editor = factory(saved, true, window, cx);
        let initial = editor.snapshot(cx);
        let owner = cx.weak_entity();
        let subscription = editor.observe(
            move |cx| {
                _ = owner.update(cx, |_, cx| cx.notify());
            },
            cx,
        );
        Self {
            editor,
            initial,
            _subscription: subscription,
        }
    }
}

fn placement_from_label(label: Option<&str>) -> Placement {
    Placement::ALL
        .into_iter()
        .find(|placement| Some(placement.label()) == label)
        .unwrap_or_default()
}

fn span_from_label(label: Option<&str>) -> BottomSpan {
    BottomSpan::ALL
        .into_iter()
        .find(|span| Some(span.label()) == label)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use subspace_dashboard::dock_panel::{BottomSpan, Placement};

    use super::{placement_from_label, span_from_label};

    #[test]
    fn select_labels_map_back_to_their_choices() {
        assert_eq!(placement_from_label(Some("Right")), Placement::Right);
        assert_eq!(placement_from_label(None), Placement::Bottom);
        assert_eq!(
            span_from_label(Some("Full window width")),
            BottomSpan::FullWidth
        );
        assert_eq!(span_from_label(Some("other")), BottomSpan::BesideChat);
    }
}
