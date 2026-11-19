//! Subspace charts: what a chart shows, and where a chart comes from.
//!
//! - [`model`] is the chart a board or an agent asks for. It has no GPUI
//!   dependency (REQ-44); `subspace_dashboard::chart` draws it.
//! - [`chartjs`] turns a Chart.js configuration into that model (REQ-43).
//! - [`flint`] compiles a Flint `ChartAssemblyInput` to Chart.js with the
//!   embedded JavaScript engine (REQ-41, REQ-42).
//! - [`guide`] is the Flint authoring guidance an agent tool returns
//!   (REQ-40).

pub mod chartjs;
pub mod flint;
pub mod model;

pub use model::{AxisScale, ChartKind, ChartModel, Orientation, Point, Series, ValueFormat};

/// How to write a Flint chart for Subspace: the chart types the Chart.js
/// backend supports, the channels each takes, and the semantic types.
///
/// Adapted from Flint's MIT-licensed `flint-chart-author` skill.
pub fn guide() -> &'static str {
    include_str!("guide.md")
}

#[cfg(test)]
mod tests {
    use crate::guide;

    #[test]
    fn the_guide_names_its_source_and_the_supported_chart_types() {
        let guide = guide();

        assert!(guide.contains("microsoft/flint-chart"), "names its source");
        assert!(guide.contains("MIT"), "names the licence");
        assert!(guide.contains("Stacked Bar Chart"));
        assert!(guide.contains("semantic_types"));
        assert!(
            !guide.contains("Vega-Lite escape hatch"),
            "the guide is cut to the Chart.js backend"
        );
    }
}
