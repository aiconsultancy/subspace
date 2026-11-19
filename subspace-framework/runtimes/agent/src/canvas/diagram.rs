use super::{CanvasContent, svg};
use mermaid_rs_renderer::{RenderOptions, Theme, parse_mermaid, render_with_options};
const MAX_BYTES: usize = 16 * 1024;
const MAX_LINES: usize = 200;
const MAX_NODES: usize = 100;
const MAX_EDGES: usize = 200;

pub(super) fn render(source: &str) -> Result<CanvasContent, String> {
    if source.len() > MAX_BYTES || source.lines().count() > MAX_LINES {
        return Err("Mermaid is limited to 16 KiB and 200 lines.".into());
    }
    let parsed = parse_mermaid(source)
        .map_err(|_| "This Mermaid syntax is unsupported. Review the source text.".to_string())?;
    if parsed.graph.nodes.len() > MAX_NODES || parsed.graph.edges.len() > MAX_EDGES {
        return Err("Mermaid is limited to 100 nodes and 200 edges.".into());
    }
    let light = themed(source, Theme::modern())?;
    let dark = themed(source, Theme::dark())?;
    Ok(CanvasContent::Svg {
        aspect: svg::aspect(&light),
        light: light.into(),
        dark: dark.into(),
    })
}

fn themed(source: &str, theme: Theme) -> Result<String, String> {
    let svg = render_with_options(
        source,
        RenderOptions {
            theme,
            ..Default::default()
        },
    )
    .map_err(|_| "This Mermaid syntax is unsupported. Review the source text.".to_string())?;
    svg::validate(&svg)?;
    Ok(svg)
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::canvas::CanvasContent;

    #[test]
    fn flowchart_and_sequence_render_without_a_browser() {
        for source in [
            "flowchart LR\n A[Read] --> B[Chart]",
            "sequenceDiagram\n participant A\n participant B\n A->>B: Query\n B-->>A: Result",
        ] {
            assert!(
                matches!(render(source), Ok(CanvasContent::Svg { .. })),
                "{source}: {:?}",
                render(source)
            );
        }
    }

    #[test]
    fn unsupported_or_oversized_sources_return_explanations() {
        assert!(render("not a diagram").is_err());
        assert!(render(&"A".repeat(16 * 1024 + 1)).is_err());
    }
}
