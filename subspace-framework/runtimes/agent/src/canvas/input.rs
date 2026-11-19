use super::{Canvas, CanvasContent, CanvasKind, MAX_INPUT_BYTES, diagram, svg};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
const TITLE_BYTES: usize = 160;
const CAPTION_BYTES: usize = 240;
const ID_BYTES: usize = 128;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    title: String,
    #[serde(default)]
    caption: String,
    source: Value,
}

pub fn prepare(id: &str, kind: &str, value: Value) -> Result<Arc<Canvas>, String> {
    validate_id(id)?;
    if value.to_string().len() > MAX_INPUT_BYTES {
        return Err("Canvas input exceeds 256 KiB.".into());
    }
    let input: Input = serde_json::from_value(value)
        .map_err(|_| "Canvas requires only title, caption and source.".to_string())?;
    validate_text(&input.title, TITLE_BYTES)?;
    validate_text(&input.caption, CAPTION_BYTES)?;
    let (kind, source, content) = content(kind, input.source)?;
    Ok(Arc::new(Canvas {
        id: id.into(),
        kind,
        title: input.title,
        caption: input.caption,
        opened_at: chrono::Local::now().format("%H:%M").to_string(),
        source,
        content,
    }))
}

pub(crate) fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > ID_BYTES
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
    {
        return Err("Canvas instance ID must be 1–128 ASCII letters, digits, dash, dot, colon or underscore.".into());
    }
    Ok(())
}

fn validate_text(text: &str, limit: usize) -> Result<(), String> {
    if text.len() > limit || text.chars().any(char::is_control) {
        return Err(format!(
            "Canvas label must be at most {limit} bytes without control characters."
        ));
    }
    Ok(())
}

fn content(kind: &str, source: Value) -> Result<(CanvasKind, String, CanvasContent), String> {
    match kind {
        "chart" => chart(source),
        "diagram" => {
            let text = source
                .as_str()
                .ok_or("Diagram source must be Mermaid text.")?
                .to_owned();
            let result = diagram::render(&text).unwrap_or_else(CanvasContent::Error);
            Ok((CanvasKind::Diagram, text, result))
        }
        "drawing" => {
            let text = source
                .as_str()
                .ok_or("Drawing source must be SVG text.")?
                .to_owned();
            svg::validate(&text)?;
            Ok((
                CanvasKind::Drawing,
                text.clone(),
                CanvasContent::Svg {
                    aspect: svg::aspect(&text),
                    light: text.clone().into(),
                    dark: text.into(),
                },
            ))
        }
        _ => Err("Unknown native canvas kind.".into()),
    }
}

fn chart(source: Value) -> Result<(CanvasKind, String, CanvasContent), String> {
    super::chart_input::validate(&source)?;
    let chartjs = subspace_charts::flint::compile_chartjs(&source)?;
    let model = subspace_charts::chartjs::to_model(&chartjs)?;
    Ok((
        CanvasKind::Chart,
        source.to_string(),
        CanvasContent::Chart(Arc::new(model)),
    ))
}

#[cfg(test)]
mod tests {
    use super::prepare;
    use serde_json::json;

    #[test]
    fn closed_schema_and_identity_limits_reject_untrusted_input() {
        assert!(prepare("../file", "drawing", json!({})).is_err());
        assert!(
            prepare(
                "id",
                "drawing",
                json!({"title":"t","source":"<svg/>","url":"file:///secret"})
            )
            .is_err()
        );
        assert!(
            prepare(
                "id",
                "drawing",
                json!({"title":"x".repeat(161),"source":"<svg/>"})
            )
            .is_err()
        );
    }

    #[test]
    fn chart_input_rejects_external_data_and_unknown_execution_fields() {
        for source in [
            json!({"data":{"url":"file:///secret"},"semantic_types":{},"chart_spec":{}}),
            json!({"data":{"values":[]},"semantic_types":{},"chart_spec":{},"execute":"read files"}),
        ] {
            let error =
                prepare("chart", "chart", json!({"title":"test","source":source})).unwrap_err();
            assert!(error.contains("Chart input"), "{error}");
        }
    }
}
