//! The designer's sections and their reset to the base theme's values.

use std::ops::Range;

use gpui_kit::component::Theme;
use gpui_kit::component::color_picker::ColorPickerEvent;
use gpui_kit::component::slider::SliderEvent;
use gpui_kit::{BorrowAppContext as _, Context, Window};
use subspace_dashboard::density::{self, Density};

use super::{Jig, colors};
use crate::theme;

/// The designer's sections, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Theme,
    Typography,
    Spacing,
    Controls,
    Colours,
    Preview,
}

impl Section {
    pub const ALL: [Self; 6] = [
        Self::Theme,
        Self::Typography,
        Self::Spacing,
        Self::Controls,
        Self::Colours,
        Self::Preview,
    ];

    pub fn title(self) -> &'static str {
        self.spec().0
    }

    /// The density groups this section edits, as indexes into `density::GROUPS`.
    pub fn density_groups(self) -> Range<usize> {
        self.spec().1
    }

    /// Sections whose values have a theme default to return to.
    pub fn is_resettable(self) -> bool {
        self.spec().2
    }

    /// The title, density groups and whether the section resets.
    fn spec(self) -> (&'static str, Range<usize>, bool) {
        let controls = density::CONTROL_GROUPS;
        match self {
            Self::Theme => ("Theme", 0..0, false),
            Self::Typography => ("Typography", 0..1, true),
            Self::Spacing => ("Spacing and density", 1..controls.start, true),
            Self::Controls => ("Controls", controls, true),
            Self::Colours => ("Colours", 0..0, true),
            Self::Preview => ("Preview and contrast", 0..0, false),
        }
    }
}

impl Jig {
    /// Return one section's values to the base theme's, as a live preview.
    pub(super) fn reset_section(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editable() {
            return;
        }
        self.reset_density(section, cx);
        match section {
            Section::Typography => self.reset_typography(window, cx),
            Section::Controls => self.reset_radii(window, cx),
            Section::Colours => self.reset_colors(window, cx),
            Section::Theme | Section::Spacing | Section::Preview => {}
        }
        theme::sync_font_sizes(cx);
        self.preview(window, cx);
    }

    fn reset_density(&self, section: Section, cx: &mut Context<Self>) {
        let base = self
            .draft
            .as_ref()
            .and_then(|document| document.metadata().ok())
            .map(|metadata| Density::from_json(&metadata.density.to_string()))
            .unwrap_or_default();
        let controls = density::GROUPS[section.density_groups()]
            .iter()
            .flat_map(|group| group.controls);
        cx.update_default_global::<Density, _>(|density, _| {
            for control in controls {
                density.set(control.token, base.get(control.token));
            }
        });
    }

    fn reset_typography(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let group = self
            .draft
            .as_ref()
            .and_then(|document| document.metadata().ok())
            .map(|metadata| metadata.group_heading)
            .unwrap_or_default();
        self.syncing = true;
        for picker in &self.fonts {
            picker.update(cx, |picker, cx| {
                picker.set_selected_value(&super::AUTOMATIC.to_owned(), window, cx)
            });
        }
        self.group_font.update(cx, |picker, cx| {
            let shown = if group.font.is_empty() {
                super::AUTOMATIC.to_owned()
            } else {
                group.font.clone()
            };
            picker.set_selected_value(&shown, window, cx)
        });
        self.group_size
            .update(cx, |slider, cx| slider.set_value(group.size, window, cx));
        self.group_bold = group.bold;
        self.group_italic = group.italic;
        self.syncing = false;
    }

    fn reset_radii(&self, window: &mut Window, cx: &mut Context<Self>) {
        for (_, _, slider, synced) in &self.radii {
            let value = *synced;
            slider.update(cx, |slider, cx| slider.set_value(value, window, cx));
        }
    }

    fn reset_colors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.syncing = true;
        for field in &mut self.colors {
            let value = field.synced.clone();
            field.show(value, window, cx);
        }
        self.syncing = false;
    }

    /// Turn a 0/1 density token, such as `--liquid-glass`, off or on.
    pub(super) fn set_switch(&mut self, token: &'static str, on: bool, cx: &mut Context<Self>) {
        let value = if on {
            density::SWITCH_ON
        } else {
            density::SWITCH_OFF
        };
        self.set_density(token, value, cx);
    }

    /// Record the sections the person opened.
    pub(super) fn set_open(&mut self, open: &[usize], cx: &mut Context<Self>) {
        self.open = open.to_vec();
        cx.notify();
    }
}

impl Jig {
    /// A colour picked or typed in a colour select previews at once.
    pub(super) fn subscribe_colors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for field in &self.colors {
            self._subscriptions.push(cx.subscribe_in(
                &field.picker,
                window,
                |jig, _, event, window, cx| {
                    let ColorPickerEvent::Change(_) = event;
                    if jig.editable() {
                        jig.preview(window, cx);
                    }
                },
            ));
        }
    }

    pub(super) fn subscribe_radii(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (_, _, slider, _) in &self.radii {
            self._subscriptions.push(cx.subscribe_in(
                slider,
                window,
                |jig, _, event, window, cx| {
                    let (SliderEvent::Change(_) | SliderEvent::Release(_)) = event;
                    if jig.editable() {
                        jig.preview(window, cx);
                    }
                },
            ));
        }
    }

    /// Preview one density token in every window. Only Save persists it.
    pub(super) fn set_density(&mut self, token: &'static str, value: f32, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        cx.update_default_global::<Density, _>(|density, _| density.set(token, value));
        theme::sync_font_sizes(cx);
        self.pending = None;
        cx.refresh_windows();
        cx.notify();
    }

    pub(super) fn sync_colors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for field in &mut self.colors {
            let value = self
                .draft
                .as_ref()
                .and_then(|document| document.color(field.key))
                .unwrap_or_else(|| colors::current(field.key, cx));
            field.show(value, window, cx);
        }
    }

    pub(super) fn sync_radii(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let theme = Theme::global(cx);
        let fallback = [f32::from(theme.radius), f32::from(theme.radius_lg)];
        for ((key, _, slider, synced), fallback) in self.radii.iter_mut().zip(fallback) {
            let value = self
                .draft
                .as_ref()
                .and_then(|document| document.radius(key))
                .unwrap_or(fallback);
            *synced = value;
            slider.update(cx, |slider, cx| slider.set_value(value, window, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Section;
    use subspace_dashboard::density::{self, GROUPS};

    #[test]
    fn every_density_token_belongs_to_exactly_one_section() {
        let mut covered: Vec<&str> = Section::ALL
            .iter()
            .flat_map(|section| GROUPS[section.density_groups()].iter())
            .flat_map(|group| group.controls.iter().map(|control| control.token))
            .collect();
        covered.sort_unstable();
        let mut all: Vec<&str> = density::controls().map(|control| control.token).collect();
        all.sort_unstable();
        assert_eq!(covered, all);
        assert_eq!(
            GROUPS[Section::Controls.density_groups()][0].name,
            "Toolbar buttons"
        );
    }
}
