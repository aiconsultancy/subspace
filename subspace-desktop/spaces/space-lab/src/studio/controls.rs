//! Preset identity, parameter, view, and bound-control validation.

use super::limits::{
    CURRENCIES, GROUP_BY, MAX_CONTROL_LABEL_LEN, MAX_CONTROLS, MAX_ID_LEN, MAX_TITLE_LEN,
    SNAPSHOT_RECIPE, VIEWS,
};
use crate::contracts::{ControlSpec, Dataset, Parameters, Preset};
use anyhow::{Context, Result, bail};
use chrono::NaiveDate;

pub fn snapshot_matches(preset: &Preset, dataset: &Dataset) -> bool {
    preset.recipe == SNAPSHOT_RECIPE
        || ((!preset.recipe.starts_with("custom:")
            || dataset
                .provenance
                .contains(&format!("recipe_ref={}", preset.recipe)))
            && dataset.provenance.contains(&format!(
                "params={}:{}:{}:{}:{}",
                preset.parameters.start,
                preset.parameters.end,
                preset.parameters.currency,
                preset.parameters.service,
                preset.parameters.group_by,
            )))
}

pub(super) fn validate_parameters(parameters: &Parameters, services: &[String]) -> Result<()> {
    let start = parse_iso_date(&parameters.start, "start")?;
    let end = parse_iso_date(&parameters.end, "end")?;
    if start > end {
        bail!("start {} is after end {}", parameters.start, parameters.end);
    }
    if !CURRENCIES.contains(&parameters.currency.as_str()) {
        bail!(
            "unsupported currency {:?}; fixture FX rates cover {}",
            parameters.currency,
            CURRENCIES.join(", ")
        );
    }
    if parameters.service != "All" && !services.contains(&parameters.service) {
        bail!(
            "unknown service {:?}; allowed: All, {}",
            parameters.service,
            services.join(", ")
        );
    }
    if !GROUP_BY.contains(&parameters.group_by.as_str()) {
        bail!(
            "unsupported group_by {:?}; allowed: {}",
            parameters.group_by,
            GROUP_BY.join(", ")
        );
    }
    Ok(())
}

pub(super) fn validate_view(view: &str) -> Result<()> {
    if VIEWS.contains(&view) {
        Ok(())
    } else {
        bail!("unsupported view {view:?}; allowed: {}", VIEWS.join(", "))
    }
}

pub(super) fn validate_title(title: &str) -> Result<()> {
    if title.trim().is_empty() {
        bail!("title is required");
    }
    if title.len() > MAX_TITLE_LEN {
        bail!("title exceeds {MAX_TITLE_LEN} bytes");
    }
    Ok(())
}

pub(super) fn parse_id(id: &str) -> Result<&str> {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        bail!("preset id must be 1 to {MAX_ID_LEN} characters");
    }
    let valid = id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        && id.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic());
    if !valid {
        bail!("preset id {id:?} is not a safe token");
    }
    Ok(id)
}

pub(super) fn validate_controls(preset: &Preset, services: &[String]) -> Result<()> {
    if preset.controls.len() > MAX_CONTROLS {
        bail!("At most five bound controls are supported");
    }
    let mut names = std::collections::HashSet::new();
    for control in &preset.controls {
        check_control(control, services, &mut names)?;
    }
    Ok(())
}

pub(super) fn add_control(preset: &mut Preset, parameter: &str, services: &[String]) -> Result<()> {
    match parameter {
        "date-range" | "date_range" => {
            push_control(preset, start_control());
            push_control(preset, end_control());
        }
        "start" => push_control(preset, start_control()),
        "end" => push_control(preset, end_control()),
        "currency" => push_control(preset, currency_control()),
        "service" => push_control(preset, service_control(services)),
        "group_by" | "group-by" => push_control(preset, group_by_control()),
        other => bail!(
            "unknown control field {other:?}; allowed: date-range, currency, service, group_by"
        ),
    }
    Ok(())
}

fn check_control<'a>(
    control: &'a ControlSpec,
    services: &[String],
    names: &mut std::collections::HashSet<&'a String>,
) -> Result<()> {
    if !names.insert(&control.parameter) {
        bail!("Duplicate parameter control");
    }
    if control.label.len() > MAX_CONTROL_LABEL_LEN || control.label.chars().any(char::is_control) {
        bail!("Invalid control label");
    }
    if control_supported(control, services) {
        Ok(())
    } else {
        bail!("Unsupported control definition: {}", control.parameter);
    }
}

fn control_supported(control: &ControlSpec, services: &[String]) -> bool {
    match control.parameter.as_str() {
        "start" | "end" => control.kind == "date" && control.options.is_empty(),
        "currency" => {
            control.kind == "select"
                && control
                    .options
                    .iter()
                    .all(|value| CURRENCIES.contains(&value.as_str()))
        }
        "service" => {
            control.kind == "select"
                && control
                    .options
                    .iter()
                    .all(|value| value == "All" || services.contains(value))
        }
        "group_by" => {
            control.kind == "select"
                && control
                    .options
                    .iter()
                    .all(|value| GROUP_BY.contains(&value.as_str()))
        }
        _ => false,
    }
}

fn parse_iso_date(value: &str, field: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("{field} must be an ISO date YYYY-MM-DD, got {value:?}"))
}

fn push_control(preset: &mut Preset, control: ControlSpec) {
    if preset
        .controls
        .iter()
        .any(|existing| existing.parameter == control.parameter)
    {
        return;
    }
    preset.controls.push(control);
}

fn start_control() -> ControlSpec {
    ControlSpec {
        parameter: "start".into(),
        label: "Start date".into(),
        kind: "date".into(),
        options: Vec::new(),
    }
}

fn end_control() -> ControlSpec {
    ControlSpec {
        parameter: "end".into(),
        label: "End date".into(),
        kind: "date".into(),
        options: Vec::new(),
    }
}

fn currency_control() -> ControlSpec {
    ControlSpec {
        parameter: "currency".into(),
        label: "Currency".into(),
        kind: "select".into(),
        options: CURRENCIES.iter().map(|item| (*item).to_owned()).collect(),
    }
}

fn service_control(services: &[String]) -> ControlSpec {
    let mut options = vec!["All".to_owned()];
    options.extend(services.iter().map(|item| (*item).to_owned()));
    ControlSpec {
        parameter: "service".into(),
        label: "Service".into(),
        kind: "select".into(),
        options,
    }
}

fn group_by_control() -> ControlSpec {
    ControlSpec {
        parameter: "group_by".into(),
        label: "Group by".into(),
        kind: "select".into(),
        options: GROUP_BY.iter().map(|item| (*item).to_owned()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::{add_control, snapshot_matches, validate_controls, validate_title};
    use crate::contracts::{ControlSpec, Dataset, Parameters, Preset};

    fn draft() -> Preset {
        Preset {
            id: "market-draft".into(),
            title: "Market".into(),
            environment: "python-data".into(),
            recipe: "market".into(),
            parameters: Parameters::default(),
            controls: Vec::new(),
            view: "bar".into(),
            snapshot: None,
            revision: 0,
            updated_at: "2026-10-24T00:00:00Z".into(),
        }
    }

    #[test]
    fn snapshot_fingerprint_tracks_parameter_tuple() {
        let preset = draft();
        let dataset = Dataset {
            columns: Vec::new(),
            rows: Vec::new(),
            labels: Vec::new(),
            series: Vec::new(),
            summary: String::new(),
            provenance: format!(
                "params={}:{}:{}:{}:{}",
                preset.parameters.start,
                preset.parameters.end,
                preset.parameters.currency,
                preset.parameters.service,
                preset.parameters.group_by
            ),
        };
        assert!(snapshot_matches(&preset, &dataset));
        let mut changed = preset;
        changed.parameters.currency = "EUR".into();
        assert!(!snapshot_matches(&changed, &dataset));
    }

    #[test]
    fn bound_controls_reject_duplicates_and_unknown_fields() {
        let mut preset = draft();
        add_control(&mut preset, "currency", &["Freight".into()]).expect("currency");
        add_control(&mut preset, "currency", &["Freight".into()]).expect("idempotent");
        assert_eq!(preset.controls.len(), 1);
        validate_controls(&preset, &["Freight".into()]).expect("valid control");
        preset.controls.push(ControlSpec {
            parameter: "currency".into(),
            label: "Again".into(),
            kind: "select".into(),
            options: vec!["USD".into()],
        });
        let duplicate = validate_controls(&preset, &["Freight".into()]).expect_err("dup");
        assert!(format!("{duplicate:#}").contains("Duplicate parameter control"));
        assert!(validate_title("").is_err());
    }
}
