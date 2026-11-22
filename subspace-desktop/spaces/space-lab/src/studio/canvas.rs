//! Stateless native-canvas export and import.

mod export;
mod import;

use super::controls::validate_title;
use crate::contracts::{Dataset, Preset};
use anyhow::{Result, bail};
use serde_json::{Value, json};

pub fn export_canvas(preset: &Preset, data: &Dataset) -> Result<Value> {
    let source = export::canvas_source(preset, data)?;
    validate_native_canvas(&source, &preset.title)?;
    Ok(source)
}

pub fn import_canvas(value: Value, title: &str) -> Result<Preset> {
    validate_title(title)?;
    let value = source_payload(value);
    validate_native_canvas(&value, title)?;
    import::preset_from_canvas(&value, title)
}

fn source_payload(value: Value) -> Value {
    value.get("source").cloned().unwrap_or(value)
}

fn validate_native_canvas(source: &Value, title: &str) -> Result<()> {
    let canvas = subspace_framework::agent::canvas::prepare(
        "space-lab-validation",
        "chart",
        json!({"title":title,"caption":"Validated space lab chart","source":source}),
    )
    .map_err(anyhow::Error::msg)?;
    if !matches!(
        &canvas.content,
        subspace_framework::agent::canvas::CanvasContent::Chart(_)
    ) {
        bail!("Canvas did not produce a native chart");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{export_canvas, import_canvas};
    use crate::contracts::{Dataset, Parameters, PlotSeries, Preset};

    fn preset() -> Preset {
        Preset {
            id: "market-canvas".into(),
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

    fn dataset() -> Dataset {
        Dataset {
            columns: vec!["location".into(), "amount".into()],
            rows: Vec::new(),
            labels: vec!["A".into(), "B".into()],
            series: vec![PlotSeries {
                name: "amount".into(),
                values: vec![1.0, 2.0],
            }],
            summary: "two points".into(),
            provenance: "params=2026-09-01:2026-09-30:USD:All:location".into(),
        }
    }

    #[test]
    fn export_round_trip_keeps_static_snapshot_and_bar_view() {
        let source = export_canvas(&preset(), &dataset()).expect("export");
        assert_eq!(source["chart_spec"]["chartType"], "Bar Chart");
        assert_eq!(source["semantic_types"]["location"], "Category");
        let imported = import_canvas(source, "Imported canvas snapshot").expect("import");
        assert_eq!(imported.recipe, "snapshot");
        assert_eq!(imported.view, "bar");
        let snapshot = imported.snapshot.expect("snapshot");
        assert_eq!(snapshot.labels, ["A", "B"]);
        assert_eq!(snapshot.series[0].values, [1.0, 2.0]);
        assert!(snapshot.provenance.contains("no-refresh-recipe"));
    }

    #[test]
    fn import_propagates_native_canvas_prepare_rejection() {
        let mut source = export_canvas(&preset(), &dataset()).expect("valid fixture");
        source["unexpected"] = serde_json::json!("not part of the chart contract");
        let error = import_canvas(source, "Rejected canvas").unwrap_err();
        assert!(
            error.to_string().contains("Chart input must contain only"),
            "{error}"
        );
    }
}
