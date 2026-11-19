//! What the inspector reports about one element, derived from GPUI's
//! inspector id and `Div` state without touching the UI.

use gpui_kit::{
    AbsoluteLength, Bounds, DefiniteLength, DivInspectorState, Edges, ElementId,
    InspectorElementId, Length, Pixels, Refineable as _, Size, Style, size,
};

/// The shared component crate; its names are written relative to it, as in
/// the UI component catalogue and the Component gallery.
const SHARED_CRATE: &str = "subspace_dashboard::";
/// Subspace-owned crates, ahead of GPUI Kit's, when naming a component.
const SUBSPACE_PREFIX: &str = "subspace_";
/// Where the shared component sources live, relative to the Framework.
const SHARED_SOURCES: &str = "board-kit/dashboard/src/";
/// Cargo's registry checkout; dependency sources are shown from their crate.
const REGISTRY_SOURCES: &str = "/registry/src/";

/// One RenderOnce component around the element. GPUI scopes each one's
/// subtree with its type name, so the element's id path lists them all.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Component {
    /// The full type path without generics, such as
    /// `subspace_dashboard::icon_button::IconButton`.
    pub(super) path: String,
}

impl Component {
    /// The catalogue name: `icon_button::IconButton` for shared components,
    /// the crate-qualified type for everything else.
    pub(super) fn name(&self) -> String {
        let path = self.path.strip_prefix(SHARED_CRATE).unwrap_or(&self.path);
        collapse_repeats(path)
    }

    fn module(&self) -> &str {
        self.path.rsplit_once("::").map_or("", |(module, _)| module)
    }

    fn is_subspace(&self) -> bool {
        self.path.starts_with(SUBSPACE_PREFIX)
    }

    fn is_shared(&self) -> bool {
        self.path.starts_with(SHARED_CRATE)
    }

    /// A component declared inside another's module, such as
    /// `icon_button::content::Glyph` inside `icon_button::IconButton`. A
    /// component at a crate's root owns no parts.
    fn is_part_of(&self, outer: &Self) -> bool {
        let module = outer.module();
        let inner = self.module();
        module.contains("::")
            && (inner == module
                || inner
                    .strip_prefix(module)
                    .is_some_and(|rest| rest.starts_with("::")))
    }
}

/// `gpui_component::button::button::Button` reads as `gpui_component::button::Button`.
fn collapse_repeats(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split("::") {
        if segments.last() != Some(&segment) {
            segments.push(segment);
        }
    }
    segments.join("::")
}

/// `crate::module::Type<…>` with an identifier for every segment, a lower
/// case crate and an upper case type: the id GPUI gives a RenderOnce scope.
fn type_path(id: &ElementId) -> Option<String> {
    let ElementId::Name(name) = id else {
        return None;
    };
    let path = name.split('<').next().unwrap_or_default();
    let segments: Vec<&str> = path.split("::").collect();
    let identifiers = segments.iter().all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
    });
    let crate_case = segments
        .first()
        .and_then(|segment| segment.chars().next())
        .is_some_and(|first| first.is_ascii_lowercase());
    let type_case = segments
        .last()
        .and_then(|segment| segment.chars().next())
        .is_some_and(|first| first.is_ascii_uppercase());
    (segments.len() > 1 && identifiers && crate_case && type_case).then(|| path.to_owned())
}

/// The components around an element, outermost first.
pub(super) fn components(path: &[ElementId]) -> Vec<Component> {
    path.iter()
        .filter_map(type_path)
        .map(|path| Component { path })
        .collect()
}

/// The component to name for an element: the innermost shared component
/// that is not a part of the one around it, then the innermost Subspace
/// component, then the innermost GPUI Kit one.
pub(super) fn named(components: &[Component]) -> Option<&Component> {
    innermost_whole(components.iter().filter(|c| c.is_shared()))
        .or_else(|| innermost_whole(components.iter().filter(|c| c.is_subspace())))
        .or_else(|| components.last())
}

/// The innermost component that is not a part of the one around it.
fn innermost_whole<'a>(list: impl Iterator<Item = &'a Component>) -> Option<&'a Component> {
    let mut named: Option<&Component> = None;
    for component in list {
        if named.is_none_or(|outer| !component.is_part_of(outer)) {
            named = Some(component);
        }
    }
    named
}

/// The nearest identified element: the last id in the path that is neither
/// a component scope nor a view.
pub(super) fn element_id(path: &[ElementId]) -> Option<String> {
    path.iter()
        .rev()
        .find(|id| type_path(id).is_none() && !matches!(id, ElementId::View(_)))
        .map(ToString::to_string)
}

/// Workspace sources stay relative; dependency sources start at their crate.
pub(super) fn compact_source(file: &str) -> String {
    match file.split_once(REGISTRY_SOURCES) {
        Some((_, rest)) => rest
            .split_once('/')
            .map_or(rest, |(_, path)| path)
            .to_owned(),
        None => file.to_owned(),
    }
}

/// The shared module a source file defines, such as `segmented` or
/// `anchored::control`, for helpers that are functions rather than types.
pub(super) fn shared_module(file: &str) -> Option<String> {
    let relative = file.split_once(SHARED_SOURCES)?.1.strip_suffix(".rs")?;
    let module = relative.strip_suffix("/mod").unwrap_or(relative);
    (module != "lib").then(|| module.replace('/', "::"))
}

/// `28 × 28`, with at most one decimal place.
pub(super) fn size_label(size: Size<Pixels>) -> String {
    format!(
        "{} × {}",
        number(f32::from(size.width)),
        number(f32::from(size.height))
    )
}

/// Sizes keep one decimal place.
const TENTHS: f32 = 10.;

fn number(value: f32) -> String {
    let rounded = (value * TENTHS).round() / TENTHS;
    if rounded.fract().abs() < f32::EPSILON {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

/// `4 8`, `4 8 2` or `1 2 3 4` in CSS shorthand order, or `0`.
pub(super) fn edges_label(edges: &Edges<Pixels>) -> String {
    let [top, right, bottom, left] =
        [edges.top, edges.right, edges.bottom, edges.left].map(|edge| number(f32::from(edge)));
    if top == right && right == bottom && bottom == left {
        top
    } else if top == bottom && right == left {
        format!("{top} {right}")
    } else if right == left {
        format!("{top} {right} {bottom}")
    } else {
        format!("{top} {right} {bottom} {left}")
    }
}

/// The element's measured box and its explicit spacing.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct BoxModel {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) content: Size<Pixels>,
    pub(super) padding: Edges<Pixels>,
    pub(super) border: Edges<Pixels>,
    pub(super) margin: Edges<Pixels>,
}

impl BoxModel {
    /// Lengths in rems use `rem_size`; fractions use the element's own size,
    /// which GPUI does not report for the parent.
    pub(super) fn new(state: &DivInspectorState, rem_size: Pixels) -> Self {
        let style = Style::default().refined((*state.base_style).clone());
        let basis = size(
            AbsoluteLength::Pixels(state.bounds.size.width),
            AbsoluteLength::Pixels(state.bounds.size.height),
        );
        Self {
            bounds: state.bounds,
            content: state.content_size,
            padding: style.padding.to_pixels(basis, rem_size),
            border: style.border_widths.to_pixels(rem_size),
            margin: margin(&style.margin, state.bounds.size, rem_size),
        }
    }
}

fn margin(edges: &Edges<Length>, basis: Size<Pixels>, rem_size: Pixels) -> Edges<Pixels> {
    let resolve = |length: Length, base: Pixels| match length {
        Length::Definite(DefiniteLength::Absolute(length)) => length.to_pixels(rem_size),
        Length::Definite(DefiniteLength::Fraction(fraction)) => base * fraction,
        Length::Auto => Pixels::ZERO,
    };
    Edges {
        top: resolve(edges.top, basis.height),
        right: resolve(edges.right, basis.width),
        bottom: resolve(edges.bottom, basis.height),
        left: resolve(edges.left, basis.width),
    }
}

/// Everything the panel and the overlay show for the active element.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ElementReport {
    pub(super) components: Vec<Component>,
    pub(super) element_id: Option<String>,
    pub(super) id_path: String,
    pub(super) source: String,
    pub(super) module: Option<String>,
    pub(super) layout: BoxModel,
}

impl ElementReport {
    pub(super) fn new(id: &InspectorElementId, state: &DivInspectorState, rem: Pixels) -> Self {
        let path: &[ElementId] = &id.path.global_id;
        let location = id.path.source_location;
        Self {
            components: components(path),
            element_id: element_id(path),
            id_path: id.path.global_id.to_string(),
            source: format!(
                "{}:{}:{}",
                compact_source(location.file()),
                location.line(),
                location.column()
            ),
            module: shared_module(location.file()),
            layout: BoxModel::new(state, rem),
        }
    }

    /// The component's catalogue name, or the shared module that built the
    /// element when no component type encloses it.
    pub(super) fn name(&self) -> Option<String> {
        named(&self.components)
            .map(Component::name)
            .or_else(|| self.module.clone())
    }

    /// The enclosing components, outermost first, as `A › B › C`.
    pub(super) fn chain(&self) -> String {
        self.components
            .iter()
            .map(Component::name)
            .collect::<Vec<_>>()
            .join(" › ")
    }

    /// The overlay tag: the name, then the size.
    pub(super) fn tag(&self) -> String {
        let size = size_label(self.layout.bounds.size);
        match self.name() {
            Some(name) => format!("{name}  {size}"),
            None => size,
        }
    }

    /// A plain-text summary for Copy all.
    pub(super) fn summary(&self) -> String {
        let layout = &self.layout;
        [
            ("Component", self.name().unwrap_or_default()),
            ("Inside", self.chain()),
            ("ID", self.element_id.clone().unwrap_or_default()),
            ("ID path", self.id_path.clone()),
            ("Source", self.source.clone()),
            ("Size", size_label(layout.bounds.size)),
            (
                "Origin",
                format!(
                    "{}, {}",
                    number(f32::from(layout.bounds.origin.x)),
                    number(f32::from(layout.bounds.origin.y))
                ),
            ),
            ("Content", size_label(layout.content)),
            ("Padding", edges_label(&layout.padding)),
            ("Border", edges_label(&layout.border)),
            ("Margin", edges_label(&layout.margin)),
        ]
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(label, value)| format!("{label}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
    }
}
