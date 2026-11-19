use roxmltree::{Document, Node, ParsingOptions};

const MAX_SVG_BYTES: usize = 512 * 1024;
const MAX_NODES: u32 = 8_000;
const MAX_DEPTH: usize = 32;
const VIEWBOX_FIELDS: usize = 4;
const MAX_DIMENSION: f64 = 4096.;
const ZERO: f64 = 0.;
const ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "text",
    "tspan",
    "title",
    "desc",
    "marker",
    "clipPath",
    "linearGradient",
    "radialGradient",
    "stop",
];
const ATTRIBUTES: &[&str] = &[
    "id",
    "class",
    "style",
    "version",
    "width",
    "height",
    "viewBox",
    "preserveAspectRatio",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "dx",
    "dy",
    "d",
    "points",
    "transform",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "opacity",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "text-anchor",
    "text-decoration",
    "dominant-baseline",
    "alignment-baseline",
    "letter-spacing",
    "marker-start",
    "marker-mid",
    "marker-end",
    "markerWidth",
    "markerHeight",
    "markerUnits",
    "refX",
    "refY",
    "orient",
    "clip-path",
    "clip-rule",
    "offset",
    "stop-color",
    "stop-opacity",
    "gradientUnits",
    "gradientTransform",
    "spreadMethod",
    "role",
    "aria-label",
    "aria-labelledby",
    "aria-roledescription",
];

pub(super) fn validate(source: &str) -> Result<(), String> {
    let document = parse(source)?;
    validate_root(&document)?;
    document
        .descendants()
        .filter(Node::is_element)
        .try_for_each(validate_node)
}

fn parse(source: &str) -> Result<Document<'_>, String> {
    validate_size(source)?;
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit: MAX_NODES,
        ..Default::default()
    };
    Document::parse_with_options(source, options)
        .map_err(|_| "Invalid or oversized SVG XML.".to_string())
}

fn validate_size(source: &str) -> Result<(), String> {
    if source.len() > MAX_SVG_BYTES {
        return Err("SVG exceeds 512 KiB.".into());
    }
    Ok(())
}

fn validate_root(document: &Document<'_>) -> Result<(), String> {
    if document.root_element().tag_name().name() != "svg" {
        return Err("Drawing root must be SVG.".into());
    }
    Ok(())
}

fn validate_node(node: Node<'_, '_>) -> Result<(), String> {
    if !ELEMENTS.contains(&node.tag_name().name()) || node.ancestors().count() > MAX_DEPTH {
        return Err("SVG contains an unsupported element or excessive nesting.".into());
    }
    for attribute in node.attributes() {
        validate_attribute(attribute.name(), attribute.value())?;
    }
    Ok(())
}

fn validate_attribute(name: &str, value: &str) -> Result<(), String> {
    validate_reference(name, value)?;
    validate_dimensions(name, value)
}

fn validate_reference(name: &str, value: &str) -> Result<(), String> {
    let lower = value.to_ascii_lowercase();
    if !ATTRIBUTES.contains(&name) && !name.starts_with("data-") {
        return Err(format!("Unsupported SVG attribute: {name}"));
    }
    if ["@", "\\", "<"].iter().any(|marker| lower.contains(marker)) {
        return Err("SVG active content and external references are forbidden.".into());
    }
    if lower.contains("url(") && !local_urls(&lower) {
        return Err("SVG references must be local #id fragments.".into());
    }
    if ["://", "data:", "file:", "javascript:"]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return Err("SVG external resources are forbidden.".into());
    }
    Ok(())
}

fn validate_dimensions(name: &str, value: &str) -> Result<(), String> {
    if matches!(name, "width" | "height") && !dimension(value) {
        return Err("SVG dimensions must be positive and at most 4096 pixels.".into());
    }
    if name == "viewBox" && !view_box(value) {
        return Err("SVG viewBox must have four finite values and dimensions at most 4096.".into());
    }
    Ok(())
}

fn view_box(value: &str) -> bool {
    let values: Vec<_> = value
        .split(|character: char| character.is_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<f64>)
        .collect();
    values.len() == VIEWBOX_FIELDS
        && values.iter().all(|value| {
            value
                .as_ref()
                .is_ok_and(|number| number.is_finite() && number.abs() <= MAX_DIMENSION)
        })
        && values[2..]
            .iter()
            .all(|value| value.as_ref().is_ok_and(|number| *number > ZERO))
}

fn local_urls(value: &str) -> bool {
    value.split("url(").skip(1).all(|part| {
        part.split_once(')').is_some_and(|(reference, _)| {
            let reference = reference.trim_matches([' ', '\'', '"']);
            reference.starts_with('#')
                && reference[1..]
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        })
    })
}

fn dimension(value: &str) -> bool {
    let number = value.trim_end_matches("px").parse::<f64>();
    number.is_ok_and(|number| number.is_finite() && number > ZERO && number <= MAX_DIMENSION)
}

/// Height over width of a validated drawing, from its `width` and `height`
/// or its `viewBox`. A drawing that names neither is treated as square.
pub(super) fn aspect(source: &str) -> f32 {
    parse(source)
        .ok()
        .and_then(|document| root_size(&document.root_element()))
        .map_or(SQUARE, |(width, height)| (height / width) as f32)
}

const SQUARE: f32 = 1.;

fn root_size(root: &Node<'_, '_>) -> Option<(f64, f64)> {
    let length = |name: &str| {
        root.attribute(name)
            .and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
            .filter(|number| *number > ZERO)
    };
    if let (Some(width), Some(height)) = (length("width"), length("height")) {
        return Some((width, height));
    }
    let view_box: Vec<f64> = root
        .attribute("viewBox")?
        .split(|character: char| character.is_whitespace() || character == ',')
        .filter_map(|part| part.parse().ok())
        .collect();
    match view_box[..] {
        [_, _, width, height] if width > ZERO && height > ZERO => Some((width, height)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{aspect, validate};

    #[test]
    fn aspect_comes_from_size_then_view_box_and_defaults_to_square() {
        assert_eq!(aspect("<svg width='400' height='200'/>"), 0.5);
        assert_eq!(aspect("<svg viewBox='0 0 100 300'/>"), 3.);
        assert_eq!(aspect("<svg width='100%' viewBox='0,0,50,25'/>"), 0.5);
        assert_eq!(aspect("<svg/>"), 1.);
        assert_eq!(aspect("not xml"), 1.);
    }

    #[test]
    fn active_external_and_oversized_drawings_are_rejected() {
        for body in [
            "<script/>",
            "<foreignObject/>",
            "<image href='file:///secret'/>",
            "<g onclick='run()'/>",
            "<path fill='url(https://host/a)'/>",
            "<rect width='99999'/>",
        ] {
            assert!(validate(&format!("<svg>{body}</svg>")).is_err(), "{body}");
        }
        assert!(validate("<!DOCTYPE svg [<!ENTITY x SYSTEM 'file:///a'>]><svg>&x;</svg>").is_err());
    }

    #[test]
    fn geometry_text_and_internal_markers_are_accepted() {
        assert!(validate("<svg width='400' height='200'><text x='10' y='20'>Hello</text><path d='M0 0 L10 10' marker-end='url(#arrow)'/></svg>").is_ok());
    }
}
