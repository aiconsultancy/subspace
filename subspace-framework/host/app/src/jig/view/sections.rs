//! One renderer per designer section. Every control previews live; the
//! footer's Save and Cancel keep their boundaries.

use gpui_kit::component::input::Input;
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Context, Div, Entity, IntoElement, ParentElement, Styled, div};
use subspace_dashboard::action::{Intent, action_button};
use subspace_dashboard::density::{self, Control, ControlGroup, Density};
use subspace_dashboard::toggle::{switch, toggle_row};
use subspace_dashboard::{Tokens, anchored};
use subspace_lucide::IconName;

use super::super::colors::{ColorField, GROUPS as COLOR_GROUPS};
use super::super::{Jig, Section};
use super::preview::{contrast_warnings, samples};

impl Jig {
    pub(super) fn section(&self, section: Section, cx: &mut Context<Self>) -> AnyElement {
        let content = match section {
            Section::Theme => self.theme_controls(),
            Section::Typography => self.typography_controls(cx),
            Section::Spacing => self.density_groups(section, cx),
            Section::Controls => self.control_settings(cx),
            Section::Colours => self.color_controls(cx),
            Section::Preview => v_flex()
                .gap_4()
                .child(samples(cx))
                .child(contrast_warnings(self.edited_document(cx), cx)),
        };
        v_flex()
            .gap_4()
            .pb_4()
            .child(content)
            .when(section.is_resettable(), |column| {
                column.child(self.reset_button(section, cx))
            })
            .into_any_element()
    }

    fn reset_button(&self, section: Section, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex().child(
            action_button(
                ("jig-reset-section", section as usize),
                Intent::Neutral,
                Some(IconName::RotateCcw),
                "Reset to theme default",
            )
            .disabled(self.busy)
            .on_click(
                cx.listener(move |jig, _, window, cx| jig.reset_section(section, window, cx)),
            ),
        )
    }

    fn theme_controls(&self) -> Div {
        v_flex()
            .gap_3()
            .child(labelled(
                "Base theme",
                anchored::select(&self.base)
                    .accessibility_label("Base theme")
                    .search_placeholder("Find a theme…")
                    .disabled(self.busy),
            ))
            .child(labelled(
                "Theme name",
                Input::new(&self.name)
                    .aria_label("Theme name")
                    .disabled(self.busy),
            ))
            .child(div().text_sm().child(
                "Use a portable file name without slashes or reserved names. Bundled theme names cannot be replaced.",
            ))
    }

    fn typography_controls(&self, cx: &mut Context<Self>) -> Div {
        let fonts = ["Headings and tabs", "Body text", "Code and logs"]
            .into_iter()
            .zip(&self.fonts)
            .map(|(label, picker)| {
                labelled(
                    label,
                    anchored::select(picker)
                        .accessibility_label(label)
                        .search_placeholder("Find an installed font…")
                        .disabled(self.busy),
                )
            });
        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_3()
                    .child(group_title("Font families"))
                    .children(fonts)
                    .child(div().text_sm().child(
                        "Automatic uses the base theme's font for that role. Body text size is the interface base: every rem-based size and control follows it.",
                    )),
            )
            .child(self.density_groups(Section::Typography, cx))
            .child(self.group_heading_controls(cx))
    }

    fn group_heading_controls(&self, cx: &mut Context<Self>) -> Div {
        v_flex()
            .gap_3()
            .child(group_title("Group headings"))
            .child(labelled(
                "Font family",
                anchored::select(&self.group_font)
                    .accessibility_label("Group heading font family")
                    .search_placeholder("Find an installed font…")
                    .disabled(self.busy),
            ))
            .child(
                v_flex()
                    .gap_2()
                    .child(h_flex().gap_2().child("Size").child(
                        subspace_dashboard::typography::format_group_heading_size(
                            self.group_size.read(cx).value().start(),
                        ),
                    ))
                    .child(Slider::new(&self.group_size).disabled(self.busy)),
            )
            .child(toggle_row(
                "Bold",
                switch("group-heading-bold", cx)
                    .checked(self.group_bold)
                    .disabled(self.busy)
                    .on_click(cx.listener(|jig, enabled: &bool, window, cx| {
                        jig.set_group_bold(*enabled, window, cx);
                    })),
            ))
            .child(toggle_row(
                "Italic",
                switch("group-heading-italic", cx)
                    .checked(self.group_italic)
                    .disabled(self.busy)
                    .on_click(cx.listener(|jig, enabled: &bool, window, cx| {
                        jig.set_group_italic(*enabled, window, cx);
                    })),
            ))
            .child(div().text_sm().child(
                "These controls affect only titles that fold or unfold row groups; icons, counts and body rows keep their own styles.",
            ))
    }

    /// The section's density groups, each under its own title.
    fn density_groups(&self, section: Section, cx: &mut Context<Self>) -> Div {
        let groups = &density::GROUPS[section.density_groups()];
        v_flex()
            .gap_4()
            .children(groups.iter().map(|group| self.density_group(group, cx)))
    }

    fn density_group(&self, group: &ControlGroup, cx: &mut Context<Self>) -> Div {
        v_flex()
            .gap_3()
            .child(group_title(group.name))
            .children(group.controls.iter().map(|control| {
                if control.is_switch() {
                    self.density_switch(control, cx).into_any_element()
                } else if control.is_choice() {
                    super::choice::density_choice(control, cx).into_any_element()
                } else {
                    self.density_slider(control, cx).into_any_element()
                }
            }))
    }

    fn density_slider(&self, control: &'static Control, cx: &App) -> Div {
        let slider = self
            .sliders
            .iter()
            .find(|(candidate, _)| candidate.token == control.token)
            .map(|(_, slider)| slider);
        let changed = cx.global::<Density>().is_changed(control.token);
        slider.map_or_else(div, |slider| {
            let value = density::format(control, slider_value(slider, cx));
            self.slider_row(control.label, slider, value, changed, cx)
        })
    }

    fn density_switch(
        &self,
        control: &'static Control,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let on = cx.global::<Density>().get(control.token) > density::SWITCH_OFF;
        let token = control.token;
        toggle_row(
            control.label,
            switch(
                gpui_kit::SharedString::from(format!("jig-switch{token}")),
                cx,
            )
            .checked(on)
            .disabled(self.busy)
            .on_click(cx.listener(move |jig, enabled: &bool, _, cx| {
                jig.set_switch(token, *enabled, cx);
            })),
        )
    }

    fn control_settings(&self, cx: &mut Context<Self>) -> Div {
        let radii: Vec<Div> = self
            .radii
            .iter()
            .map(|(_, label, slider, _)| {
                let value = format!("{}px", slider_value(slider, cx));
                self.slider_row(label, slider, value, false, cx)
            })
            .collect();
        v_flex()
            .gap_4()
            .child(div().text_sm().child(
                "Button sizes are pixels at 13 px body text and scale with it. A radius over half the height draws a capsule.",
            ))
            .child(self.density_groups(Section::Controls, cx))
            .child(
                v_flex()
                    .gap_3()
                    .child(group_title("GPUI Kit fields and surfaces"))
                    .children(radii),
            )
    }

    fn color_controls(&self, cx: &App) -> Div {
        v_flex()
            .gap_4()
            .children(COLOR_GROUPS.iter().map(|(group, fields)| {
                v_flex()
                    .gap_3()
                    .child(group_title(group))
                    .children(fields.iter().filter_map(|(key, _)| {
                        let field = self.colors.iter().find(|field| field.key == *key)?;
                        Some(color_row(field))
                    }))
            }))
            .child(contrast_warnings(self.edited_document(cx), cx))
    }
}

fn slider_value(slider: &Entity<SliderState>, cx: &App) -> f32 {
    slider.read(cx).value().start()
}

impl Jig {
    /// A labelled slider; the value shows in the accent colour once it
    /// differs from the built-in default.
    fn slider_row(
        &self,
        label: &'static str,
        slider: &Entity<SliderState>,
        value: String,
        changed: bool,
        cx: &App,
    ) -> Div {
        let tokens = Tokens::current(cx);
        v_flex()
            .gap_2()
            .child(
                h_flex().gap_2().child(label).child(
                    div()
                        .text_color(if changed { tokens.accent } else { tokens.muted })
                        .child(value),
                ),
            )
            .child(Slider::new(slider).disabled(self.busy))
    }
}

fn color_row(field: &ColorField) -> Div {
    labelled(
        field.label,
        anchored::color_select(&field.picker).accessibility_label(field.label),
    )
}

fn group_title(title: &'static str) -> Div {
    div()
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .child(title)
}

pub(super) fn labelled(label: &'static str, control: impl IntoElement) -> Div {
    v_flex().min_w_0().gap_1().child(label).child(control)
}
