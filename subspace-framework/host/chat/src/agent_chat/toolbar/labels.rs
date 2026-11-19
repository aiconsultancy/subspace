//! The text and the choices behind the toolbar menus: plain functions over
//! the view model, so the panel's behaviour is testable without a window.

use gpui_kit::SharedString;

use super::{ChatModeOption, ModelOption, ToolbarEvent, Usage};

/// The label of the Reasoning entry that leaves the effort to the model.
pub const DEFAULT_EFFORT: &str = "Default";
const USAGE_PLACEHOLDER: &str = "No request yet.";
/// Nano-AI units per AI unit.
const NANO: f64 = 1e9;
const THOUSAND: f64 = 1e3;
const MILLION: f64 = 1e6;

pub fn mode_label(mode: ChatModeOption) -> &'static str {
    match mode {
        ChatModeOption::Interactive => "Interactive",
        ChatModeOption::Plan => "Plan",
        ChatModeOption::Autopilot => "Autopilot",
    }
}

/// Always ask, or approve everything (YOLO).
pub fn permission_label(approve_all: bool) -> &'static str {
    if approve_all {
        "Approve all"
    } else {
        "Always ask"
    }
}

/// The reasoning entries for `model`: Default first, then its own efforts.
/// A model without efforts offers nothing to choose, so the control hides.
pub fn effort_options(model: Option<&ModelOption>) -> Vec<SharedString> {
    let efforts = model.map(|model| model.efforts.as_slice()).unwrap_or(&[]);
    if efforts.is_empty() {
        return Vec::new();
    }
    std::iter::once(SharedString::from(DEFAULT_EFFORT))
        .chain(efforts.iter().cloned())
        .collect()
}

/// The model whose id is `id`.
pub fn current_model<'a>(models: &'a [ModelOption], id: Option<&str>) -> Option<&'a ModelOption> {
    let id = id?;
    models.iter().find(|model| model.id == id)
}

/// The Model button's label.
pub fn model_label(model: Option<&ModelOption>) -> SharedString {
    model.map_or_else(|| "Model".into(), |model| model.label.clone())
}

/// The Reasoning button's label: the chosen effort, else Default.
pub fn effort_label(effort: Option<&str>) -> SharedString {
    match effort {
        Some(effort) if !effort.is_empty() => effort.to_owned().into(),
        _ => DEFAULT_EFFORT.into(),
    }
}

/// Whether `effort` is the checked entry of the Reasoning menu.
pub fn effort_checked(effort: &SharedString, chosen: Option<&str>) -> bool {
    match chosen {
        Some(chosen) => effort == chosen,
        None => effort == DEFAULT_EFFORT,
    }
}

/// `Default` means the model decides, so no effort is sent.
pub fn effort_value(effort: &SharedString) -> Option<SharedString> {
    (effort != DEFAULT_EFFORT).then(|| effort.clone())
}

/// The one line in the usage popover, such as `5.2k in · 7 out · 0.058 AI
/// units`. Missing counters are left out; nothing is invented.
pub fn usage_line(usage: &Usage, show_ai_units: bool) -> SharedString {
    let mut parts = Vec::new();
    if let Some(input) = usage.input_tokens {
        parts.push(format!("{} in", tokens(input)));
    }
    if let Some(output) = usage.output_tokens {
        parts.push(format!("{} out", tokens(output)));
    }
    if show_ai_units && let Some(nano) = usage.nano_aiu {
        parts.push(format!("{} AI units", units(nano)));
    }
    if parts.is_empty() {
        return USAGE_PLACEHOLDER.into();
    }
    parts.join(" · ").into()
}

/// A token count, shortened to `k` above a thousand and `M` above a million.
fn tokens(count: u64) -> String {
    let count = count as f64;
    if count >= MILLION {
        return format!("{:.1}M", count / MILLION);
    }
    if count >= THOUSAND {
        return format!("{:.1}k", count / THOUSAND);
    }
    format!("{count:.0}")
}

/// Nano-AI units as AI units, with enough digits to stay non-zero.
fn units(nano: f64) -> String {
    const SMALL: f64 = 0.001;
    const ZERO: f64 = 0.;
    let value = nano / NANO;
    if value != ZERO && value.abs() < SMALL {
        return format!("{value:.6}");
    }
    format!("{value:.3}")
}

/// The model list's sections, in render order: starred models, then the
/// rest, each in the SDK's order.
pub fn model_sections(models: &[ModelOption], favourites: &[String]) -> [Vec<ModelOption>; 2] {
    let (starred, rest) = models
        .iter()
        .cloned()
        .partition(|model| is_favourite(favourites, &model.id));
    [starred, rest]
}

pub fn is_favourite(favourites: &[String], id: &str) -> bool {
    favourites.iter().any(|favourite| favourite == id)
}

/// Star `id`, or unstar it if it is already a favourite.
pub fn toggle_favourite(favourites: &mut Vec<String>, id: &str) {
    match favourites.iter().position(|favourite| favourite == id) {
        Some(index) => {
            favourites.remove(index);
        }
        None => favourites.push(id.to_owned()),
    }
}

/// The model behind a confirmed row of the model list.
pub fn model_choice(
    sections: &[Vec<ModelOption>],
    section: usize,
    row: usize,
) -> Option<ToolbarEvent> {
    sections
        .get(section)?
        .get(row)
        .map(|model| ToolbarEvent::SetModel {
            model: model.id.clone(),
        })
}

#[cfg(test)]
mod tests {
    use super::super::{ChatModeOption, ModelOption, ToolbarEvent, Usage};
    use super::{
        DEFAULT_EFFORT, current_model, effort_checked, effort_label, effort_options, effort_value,
        is_favourite, mode_label, model_choice, model_label, model_sections, permission_label,
        toggle_favourite, usage_line,
    };
    use gpui_kit::SharedString;

    fn models() -> Vec<ModelOption> {
        vec![
            ModelOption {
                id: "gpt-5".into(),
                label: "GPT-5".into(),
                efforts: vec!["low".into(), "high".into()],
            },
            ModelOption {
                id: "haiku".into(),
                label: "Claude Haiku".into(),
                efforts: Vec::new(),
            },
        ]
    }

    #[test]
    fn reasoning_lists_default_and_only_the_models_own_efforts() {
        let models = models();
        let efforts = effort_options(current_model(&models, Some("gpt-5")));
        assert_eq!(efforts, [DEFAULT_EFFORT, "low", "high"]);
        assert!(
            effort_options(current_model(&models, Some("haiku"))).is_empty(),
            "a model without efforts hides the control"
        );
        assert!(effort_options(current_model(&models, None)).is_empty());
        assert!(effort_options(current_model(&models, Some("gone"))).is_empty());
    }

    #[test]
    fn the_reasoning_control_checks_and_reports_default_without_an_effort() {
        let default: SharedString = DEFAULT_EFFORT.into();
        let high: SharedString = "high".into();
        assert_eq!(effort_label(None), DEFAULT_EFFORT);
        assert_eq!(effort_label(Some("high")), "high");
        assert!(effort_checked(&default, None));
        assert!(!effort_checked(&high, None));
        assert!(effort_checked(&high, Some("high")));
        assert_eq!(effort_value(&default), None);
        assert_eq!(effort_value(&high), Some(high.clone()));
    }

    #[test]
    fn the_model_button_names_the_chosen_model() {
        let models = models();
        assert_eq!(model_label(current_model(&models, Some("gpt-5"))), "GPT-5");
        assert_eq!(model_label(None), "Model");
        let sections = model_sections(&models, &[]);
        assert_eq!(
            model_choice(&sections, 1, 1),
            Some(ToolbarEvent::SetModel {
                model: "haiku".into()
            })
        );
        assert_eq!(model_choice(&sections, 1, 9), None);
        assert_eq!(model_choice(&sections, 0, 0), None);
    }

    #[test]
    fn starred_models_come_first_and_choose_from_their_own_section() {
        let models = models();
        let mut favourites = Vec::new();
        toggle_favourite(&mut favourites, "haiku");
        let [starred, rest] = model_sections(&models, &favourites);
        assert_eq!(starred[0].id, "haiku");
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].id, "gpt-5");
        let sections = [starred, rest];
        assert_eq!(
            model_choice(&sections, 0, 0),
            Some(ToolbarEvent::SetModel {
                model: "haiku".into()
            })
        );
        toggle_favourite(&mut favourites, "haiku");
        assert!(!is_favourite(&favourites, "haiku"));
    }

    #[test]
    fn menu_labels() {
        assert_eq!(mode_label(ChatModeOption::Interactive), "Interactive");
        assert_eq!(mode_label(ChatModeOption::Plan), "Plan");
        assert_eq!(mode_label(ChatModeOption::Autopilot), "Autopilot");
        assert_eq!(permission_label(false), "Always ask");
        assert_eq!(permission_label(true), "Approve all");
    }

    #[test]
    fn usage_shows_one_compact_line_and_omits_what_is_missing() {
        assert_eq!(
            usage_line(
                &Usage {
                    input_tokens: Some(5163),
                    output_tokens: Some(7),
                    nano_aiu: Some(58_392_000.),
                },
                true
            ),
            "5.2k in · 7 out · 0.058 AI units"
        );
        assert_eq!(
            usage_line(
                &Usage {
                    input_tokens: Some(2_500_000),
                    output_tokens: None,
                    nano_aiu: None,
                },
                true
            ),
            "2.5M in"
        );
        assert_eq!(
            usage_line(
                &Usage {
                    input_tokens: None,
                    output_tokens: None,
                    nano_aiu: Some(12_000.),
                },
                true
            ),
            "0.000012 AI units",
            "a tiny cost keeps digits instead of rounding to zero"
        );
        assert_eq!(usage_line(&Usage::default(), true), "No request yet.");
        assert_eq!(
            usage_line(
                &Usage {
                    input_tokens: Some(10),
                    output_tokens: Some(4),
                    nano_aiu: Some(1_000_000.),
                },
                false,
            ),
            "10 in · 4 out"
        );
    }
}
