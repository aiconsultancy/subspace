//! Transactional appearance drafts. Only Save changes files or preferences.

mod colors;
mod save;
mod sections;
#[cfg(test)]
mod tests;
mod view;

use gpui_kit::component::color_picker::ColorPickerState;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::select::{SearchableVec, SelectEvent, SelectState};
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::{
    App, AppContext as _, BorrowAppContext as _, Context, Entity, EventEmitter, FocusHandle,
    ScrollHandle, SharedString, Subscription, Window,
};
use subspace_dashboard::density::{self, Control, Density, DensityPreview};

use crate::theme::GroupHeadingChoices;
use crate::theme::{self, AppearanceSnapshot, FontChoices, KIT_RADII, ThemeDocument};
use colors::ColorField;
pub(crate) use sections::Section;

pub const JIG_CONTEXT: &str = "SubspaceJig";
const AUTOMATIC: &str = theme::AUTOMATIC_FONT;
const GROUP_HEADING_SIZE_STEP: f32 = 0.5;
type Choice = Entity<SelectState<SearchableVec<String>>>;

const RADIUS_MIN: f32 = 0.;
const RADIUS_MAX: f32 = 24.;
/// A GPUI Kit radius slider: key, label, slider and the value at the last sync.
type RadiusField = (&'static str, &'static str, Entity<SliderState>, f32);
const RADIUS_STEP: f32 = 1.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JigEvent {
    Close,
}

pub struct Jig {
    base: Choice,
    fonts: [Choice; 3],
    group_font: Choice,
    group_size: Entity<SliderState>,
    group_bold: bool,
    group_italic: bool,
    name: Entity<InputState>,
    colors: Vec<ColorField>,
    sliders: Vec<(&'static Control, Entity<SliderState>)>,
    /// GPUI Kit's radii and the value each showed after the last sync.
    radii: Vec<RadiusField>,
    /// The open sections, as indexes into [`Section::ALL`].
    open: Vec<usize>,
    draft: Option<ThemeDocument>,
    snapshot: Option<AppearanceSnapshot>,
    pending: Option<save::PendingSave>,
    busy: bool,
    syncing: bool,
    generation: u64,
    focus: FocusHandle,
    scroll: ScrollHandle,
    status: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<JigEvent> for Jig {}

impl Jig {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let selected = theme::selected_theme(cx);
        let base = choice(theme::theme_names(cx), &selected, window, cx);
        let fonts = std::array::from_fn(|_| choice(theme::font_choices(cx), AUTOMATIC, window, cx));
        let group_font = choice(theme::font_choices(cx), AUTOMATIC, window, cx);
        let group_size = cx.new(|_| {
            SliderState::new()
                .min(subspace_dashboard::typography::GROUP_HEADING_SIZE_MIN)
                .max(subspace_dashboard::typography::GROUP_HEADING_SIZE_MAX)
                .step(GROUP_HEADING_SIZE_STEP)
                .default_value(subspace_dashboard::typography::GROUP_HEADING_SIZE_DEFAULT)
        });
        let name = input("My theme", window, cx);
        let colors = color_fields(window, cx);
        let radii = radius_sliders(cx);
        let sliders = density_sliders(cx);
        let mut jig = Self {
            base,
            fonts,
            group_font,
            group_size,
            group_bold: true,
            group_italic: false,
            name,
            colors,
            sliders,
            radii,
            open: vec![Section::Theme as usize],
            draft: None,
            snapshot: None,
            pending: None,
            busy: false,
            syncing: false,
            generation: 0,
            focus: cx.focus_handle(),
            status: SharedString::default(),
            _subscriptions: Vec::new(),
            scroll: ScrollHandle::default(),
        };
        jig.subscribe(window, cx);
        jig
    }

    fn subscribe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._subscriptions.push(cx.subscribe_in(
            &self.base,
            window,
            |jig, _, event, window, cx| {
                if matches!(event, SelectEvent::Confirm(Some(_))) && jig.editable() {
                    jig.load_base(window, cx);
                }
            },
        ));
        for font in &self.fonts {
            self._subscriptions
                .push(cx.subscribe_in(font, window, |jig, _, event, window, cx| {
                    if matches!(event, SelectEvent::Confirm(Some(_))) && jig.editable() {
                        jig.preview(window, cx);
                    }
                }));
        }
        self.subscribe_group_heading(window, cx);
        self.subscribe_colors(window, cx);
        self.subscribe_density(window, cx);
        self.subscribe_radii(window, cx);
    }

    fn subscribe_group_heading(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._subscriptions.push(cx.subscribe_in(
            &self.group_font,
            window,
            |jig, _, event, window, cx| {
                if matches!(event, SelectEvent::Confirm(Some(_))) && jig.editable() {
                    jig.preview(window, cx);
                }
            },
        ));
        self._subscriptions.push(cx.subscribe_in(
            &self.group_size,
            window,
            |jig, _, event, window, cx| {
                let (SliderEvent::Change(_) | SliderEvent::Release(_)) = event;
                if jig.editable() {
                    jig.preview(window, cx);
                }
            },
        ));
    }

    fn subscribe_density(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._subscriptions
            .push(cx.subscribe(&self.name, |jig, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    jig.pending = None;
                    cx.notify();
                }
            }));
        for (control, slider) in &self.sliders {
            let control = *control;
            self._subscriptions.push(cx.subscribe_in(
                slider,
                window,
                move |jig, _, event, _, cx| {
                    let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
                    jig.set_density(control.token, value.start(), cx);
                },
            ));
        }
    }

    fn editable(&self) -> bool {
        self.snapshot.is_some() && !self.busy && !self.syncing
    }

    pub fn can_close(&self) -> bool {
        !self.busy
    }

    pub fn begin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.snapshot = Some(AppearanceSnapshot::capture(cx));
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        let selected = theme::selected_theme(cx);
        self.status = theme::font_warnings(&selected, &theme::selected_fonts(cx), cx)
            .unwrap_or_else(|error| vec![error])
            .join("\n")
            .into();
        self.base.update(cx, |base, cx| {
            base.set_selected_value(&selected, window, cx)
        });
        self.draft = theme::document(&selected, cx).ok();
        self.sync_fields(&theme::selected_fonts(cx), window, cx);
        cx.set_global(DensityPreview(true));
        self.focus.focus(window, cx);
    }

    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(snapshot) = self.snapshot.take() {
            snapshot.restore(cx);
        }
        self.pending = None;
        self.generation = self.generation.wrapping_add(1);
        window.refresh();
    }

    fn load_base(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.base.read(cx).selected_value().cloned() else {
            return;
        };
        match theme::document(&name, cx) {
            Ok(document) => {
                if let Err(error) = theme::apply_document(&document, &FontChoices::default(), cx) {
                    self.status = error.into();
                    return;
                }
                self.draft = Some(document);
                self.sync_fields(&FontChoices::default(), window, cx);
                self.preview(window, cx);
            }
            Err(error) => self.status = error.into(),
        }
        cx.notify();
    }

    fn sync_fields(&mut self, fonts: &FontChoices, window: &mut Window, cx: &mut Context<Self>) {
        self.syncing = true;
        for (picker, value) in self
            .fonts
            .iter()
            .zip([&fonts.heading, &fonts.body, &fonts.mono])
        {
            let shown = if value.is_empty() {
                AUTOMATIC
            } else {
                value.as_str()
            };
            picker.update(cx, |picker, cx| {
                picker.set_selected_value(&shown.to_owned(), window, cx)
            });
        }
        let group = self
            .draft
            .as_ref()
            .and_then(|document| document.metadata().ok())
            .map(|metadata| metadata.group_heading)
            .unwrap_or_default();
        let shown = if group.font.is_empty() {
            AUTOMATIC
        } else {
            group.font.as_str()
        };
        self.group_font.update(cx, |picker, cx| {
            picker.set_selected_value(&shown.to_owned(), window, cx)
        });
        self.group_size
            .update(cx, |slider, cx| slider.set_value(group.size, window, cx));
        self.group_bold = group.bold;
        self.group_italic = group.italic;
        self.sync_colors(window, cx);
        self.sync_radii(window, cx);
        self.syncing = false;
        self.pending = None;
    }

    fn chosen_fonts(&self, cx: &App) -> FontChoices {
        let [heading, body, mono] = self.fonts.each_ref().map(|picker| {
            picker
                .read(cx)
                .selected_value()
                .filter(|font| font.as_str() != AUTOMATIC)
                .cloned()
                .unwrap_or_default()
        });
        FontChoices {
            heading,
            body,
            mono,
        }
    }

    fn chosen_group_heading(&self, cx: &App) -> GroupHeadingChoices {
        GroupHeadingChoices {
            font: self
                .group_font
                .read(cx)
                .selected_value()
                .filter(|font| font.as_str() != AUTOMATIC)
                .cloned()
                .unwrap_or_default(),
            size: self.group_size.read(cx).value().start(),
            bold: self.group_bold,
            italic: self.group_italic,
        }
    }

    fn set_group_bold(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.group_bold = enabled;
        self.preview(window, cx);
    }

    fn set_group_italic(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.group_italic = enabled;
        self.preview(window, cx);
    }

    fn sync_sliders(&self, window: &mut Window, cx: &mut Context<Self>) {
        let density = cx.global::<Density>().clone();
        for (control, slider) in &self.sliders {
            let value = density.get(control.token);
            if (slider.read(cx).value().start() - value).abs() > f32::EPSILON {
                slider.update(cx, |slider, cx| slider.set_value(value, window, cx));
            }
        }
    }

    fn edited_document(&self, cx: &App) -> Result<ThemeDocument, String> {
        let mut document = self.draft.clone().ok_or("Select a base theme")?;
        for field in &self.colors {
            let value = field.value(cx);
            // A colour the base theme derives stays derived until it is edited.
            if document.color(field.key).is_some() || value != field.synced {
                document
                    .set_color(field.key, &value)
                    .map_err(|error| format!("{}: {error}", field.label))?;
            }
        }
        // GPUI Kit keeps its last radius when a theme omits one, so the draft
        // always states both; otherwise a reset could not take effect.
        for (key, _, slider, _) in &self.radii {
            document.set_radius(key, radius_pixels(slider.read(cx).value().start()))?;
        }
        let mut metadata = document.metadata()?;
        metadata.fonts = theme::effective_fonts(&self.chosen_fonts(cx), &metadata.fonts);
        metadata.group_heading = self.chosen_group_heading(cx);
        metadata.density = serde_json::from_str(&cx.global::<Density>().to_json())
            .map_err(|error| error.to_string())?;
        document.set_metadata(metadata)?;
        Ok(document)
    }

    fn preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending = None;
        let result = self.edited_document(cx).and_then(|document| {
            let warnings = theme::apply_document(&document, &FontChoices::default(), cx)?;
            Ok(warnings.join("\n"))
        });
        self.status = result.unwrap_or_else(|error| error).into();
        window.refresh();
        cx.notify();
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.busy {
            self.load_base(window, cx);
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let result = self
            .named_document(cx)
            .and_then(|(_, document)| document.to_json());
        match result {
            Ok(json) => {
                subspace_dashboard::links::copy(json, cx);
                self.status = "Theme JSON copied. No files saved.".into();
            }
            Err(error) => self.status = error.into(),
        }
        cx.notify();
    }
}

fn input(value: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| {
        let mut input = InputState::new(window, cx);
        input.set_value(value.to_owned(), window, cx);
        input
    })
}

fn choice(items: Vec<String>, selected: &str, window: &mut Window, cx: &mut App) -> Choice {
    cx.new(|cx| {
        let mut state =
            SelectState::new(SearchableVec::new(items), None, window, cx).searchable(true);
        state.set_selected_value(&selected.to_owned(), window, cx);
        state
    })
}

/// A slider's radius as GPUI Kit's whole pixels.
fn radius_pixels(value: f32) -> u16 {
    value.round().clamp(RADIUS_MIN, RADIUS_MAX) as u16
}

fn color_fields(window: &mut Window, cx: &mut App) -> Vec<ColorField> {
    colors::keys()
        .map(|(key, label)| ColorField {
            key,
            label,
            picker: cx.new(|cx| ColorPickerState::new(window, cx)),
            synced: String::new(),
        })
        .collect()
}

fn radius_sliders(cx: &mut App) -> Vec<RadiusField> {
    KIT_RADII
        .iter()
        .map(|(key, label)| {
            let slider = cx.new(|_| SliderState::new().max(RADIUS_MAX).step(RADIUS_STEP));
            (*key, *label, slider, RADIUS_MIN)
        })
        .collect()
}

fn density_sliders(cx: &mut App) -> Vec<(&'static Control, Entity<SliderState>)> {
    density::controls()
        .filter(|control| !control.is_switch() && !control.is_choice())
        .map(|control| {
            let slider = cx.new(|_| {
                SliderState::new()
                    .max(control.max)
                    .min(control.min)
                    .step(control.step)
                    .default_value(control.default)
            });
            (control, slider)
        })
        .collect()
}
