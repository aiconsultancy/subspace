//! Board feature controls and app-only Application Experiments composition.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, Context, Div, IntoElement, ParentElement as _, Styled as _};
use subspace_core::features::{FeatureSpec, FeatureStage};
use subspace_dashboard::Tokens;
use subspace_dashboard::settings::SettingsSection;
use subspace_dashboard::toggle::toggle_row;

use super::super::{Settings, note};
use crate::settings::APP_FEATURES;

/// Application Experiments never enumerates board registrations.
pub(super) fn app_experiments(
    settings: &Settings,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> Option<Div> {
    let features = features_at_stage(APP_FEATURES, FeatureStage::Experimental);
    (!features.is_empty()).then(|| {
        v_flex()
            .gap_2()
            .child(super::super::heading("App", cx))
            .children(features.iter().map(|feature| {
                feature_row(settings, feature, false, tokens, cx).into_any_element()
            }))
    })
}

/// The stable features' rows; `locked` disables them while their board is
/// turned off.
pub(in crate::settings::view) fn stable_feature_rows(
    settings: &Settings,
    features: &'static [FeatureSpec],
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> Vec<AnyElement> {
    features_at_stage(features, FeatureStage::Stable)
        .into_iter()
        .map(|feature| feature_row(settings, feature, locked, tokens, cx).into_any_element())
        .collect()
}

pub(super) fn experiment_footer(
    settings: &Settings,
    features: &'static [FeatureSpec],
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> Option<Div> {
    let experiments = features_at_stage(features, FeatureStage::Experimental);
    experiment_footer_items(settings, &experiments, locked, tokens, cx)
}

pub(super) fn experiment_footer_items(
    settings: &Settings,
    experiments: &[&'static FeatureSpec],
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> Option<Div> {
    (!experiments.is_empty()).then(|| {
        v_flex()
            .gap_2()
            .mt_2()
            .child(super::super::heading("Experiments", cx))
            .children(
                experiments
                    .iter()
                    .map(|feature| feature_row(settings, feature, locked, tokens, cx)),
            )
    })
}

pub(super) fn board_experiments(
    settings: subspace_dashboard::settings::BoardSettings,
) -> Vec<&'static FeatureSpec> {
    board_features_at_stage(settings, FeatureStage::Experimental)
}

fn feature_row(
    settings: &Settings,
    feature: &'static FeatureSpec,
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> Div {
    let enabled = settings.form.feature_enabled(feature);
    let control = if feature.toggleable {
        toggle_row(
            feature.title,
            subspace_dashboard::toggle::switch(format!("feature-{}", feature.id), cx)
                .checked(enabled)
                .disabled(locked)
                .on_click(cx.listener(move |settings, enabled: &bool, window, cx| {
                    settings.toggle_feature(feature, *enabled, window, cx);
                })),
        )
    } else {
        h_flex()
            .gap_3()
            .child(gpui_kit::div().flex_1().child(feature.title))
            .child(note(
                tokens,
                if enabled {
                    "Always enabled"
                } else {
                    "Unavailable"
                },
            ))
    };
    v_flex()
        .gap_1()
        .py_2()
        .border_b_1()
        .border_color(tokens.line)
        .child(control)
        .child(note(tokens, feature.description))
}

pub(super) fn features_at_stage(
    features: &'static [FeatureSpec],
    stage: FeatureStage,
) -> Vec<&'static FeatureSpec> {
    features
        .iter()
        .filter(|feature| feature.stage == stage)
        .collect()
}

pub(super) fn board_features_at_stage(
    settings: subspace_dashboard::settings::BoardSettings,
    stage: FeatureStage,
) -> Vec<&'static FeatureSpec> {
    let mut features = features_at_stage(settings.features, stage);
    collect_section_features(settings.sections, stage, &mut features);
    features
}

fn collect_section_features(
    sections: &'static [SettingsSection],
    stage: FeatureStage,
    output: &mut Vec<&'static FeatureSpec>,
) {
    for section in sections {
        output.extend(features_at_stage(section.features, stage));
        collect_section_features(section.children, stage, output);
    }
}
