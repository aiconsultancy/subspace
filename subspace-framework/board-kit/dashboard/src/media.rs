//! Shared media surfaces. Owners supply private decoded pixels and native actions.
use crate::Tokens;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::attachment::{
    Attachment, AttachmentContent, AttachmentMedia, AttachmentStatus, AttachmentTitle,
};
use gpui_kit::component::v_flex;
use gpui_kit::{
    Div, ElementId, InteractiveElement as _, ParentElement as _, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, rems,
};

const TILE_WIDTH_REM: f32 = 13.;
const TILE_MIN_HEIGHT_REM: f32 = 8.75;

/// A preview tile; image decoding, viewport admission and commands stay with its owner.
pub fn tile(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tokens: &Tokens,
) -> Stateful<Div> {
    v_flex()
        .id(id)
        .aria_label(label)
        .relative()
        .w(rems(TILE_WIDTH_REM))
        .min_h(rems(TILE_MIN_HEIGHT_REM))
        .gap_1()
        .p_2()
        .rounded(tokens.control_radius)
        .border_1()
        .border_color(tokens.line)
        .bg(tokens.pane)
}

/// A quiet, read-only file label with metadata supplied by its owner.
pub fn file_chip(label: impl Into<SharedString>, tokens: &Tokens) -> Div {
    div()
        .px_2()
        .py_1()
        .rounded(tokens.control_radius)
        .bg(tokens.pane)
        .border_1()
        .border_color(tokens.line)
        .child(label.into())
}

/// What a received attachment's thumbnail shows.
pub enum Thumb {
    /// Decoded pixels, owned and bounded by the caller: a square tile.
    Image(std::sync::Arc<gpui_kit::RenderImage>),
    /// Markdown or plain text: a compact chip with the name.
    Text,
    /// Any other file: a compact chip with the name.
    File,
    /// Still resolving: a chip with a spinner.
    Loading,
    /// Couldn't load for now: a failed chip that stays clickable to retry.
    Retry,
}

/// The side of a received image tile.
const THUMB_REM: f32 = 6.;
const KB: f64 = 1000.;

/// A size as people read it, in decimal units like the upload limit:
/// `512 bytes`, `1.5 KB`, `3.2 MB`.
pub fn size_text(bytes: usize) -> String {
    let value = bytes as f64;
    if value < KB {
        format!("{bytes} bytes")
    } else if value < KB * KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{:.1} MB", value / (KB * KB))
    }
}

/// A received attachment: GPUI Kit's `Attachment` (a square tile for an
/// image, a chip for any other file) inside a wrapper that carries the
/// button role and `label`, since `Attachment` has neither focus nor an
/// accessible name. The owner's keyboard cursor sets `ring`, drawn as an
/// overlay so it never moves the layout. `tooltip` shows on hover; a
/// click runs `open`.
///
/// The wrapper takes no keyboard focus of its own: the owner provides
/// keyboard focus and activation, such as a row cursor that sets `ring`
/// and runs `open` on Enter.
pub fn thumbnail(
    id: impl Into<ElementId>,
    thumb: Thumb,
    (name, label, tooltip): (SharedString, SharedString, SharedString),
    (ring, open): (
        bool,
        impl Fn(&mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
    ),
    tokens: &Tokens,
) -> impl gpui_kit::IntoElement {
    let id = id.into();
    let card = attachment_card(
        ElementId::from((id.clone(), "attachment")),
        thumb,
        name,
        tooltip,
    );
    div()
        .id(id)
        .relative()
        .flex_none()
        .cursor_pointer()
        .role(gpui_kit::accesskit::Role::Button)
        .aria_label(label)
        .on_click(move |_, window, cx| open(window, cx))
        .child(card)
        .children(ring.then(|| focus_ring(tokens)))
        .test_support()
}

/// The Kit card for `thumb`: a large vertical tile with the image, or a
/// small chip with an icon, status and `name`.
fn attachment_card(
    id: ElementId,
    thumb: Thumb,
    name: SharedString,
    tooltip: SharedString,
) -> Attachment {
    dress(Attachment::new().id(id).tooltip(tooltip), face(thumb), name)
}

/// The image of an image thumbnail, or the icon and status of a chip.
fn face(
    thumb: Thumb,
) -> Result<std::sync::Arc<gpui_kit::RenderImage>, (subspace_lucide::IconName, AttachmentStatus)> {
    use subspace_lucide::IconName;
    match thumb {
        Thumb::Image(image) => Ok(image),
        Thumb::Text => Err((IconName::FileText, AttachmentStatus::Complete)),
        Thumb::Loading => Err((IconName::File, AttachmentStatus::Processing)),
        Thumb::Retry => Err((IconName::FileX, AttachmentStatus::Failed)),
        Thumb::File => Err((IconName::File, AttachmentStatus::Complete)),
    }
}

fn dress(
    card: Attachment,
    face: Result<
        std::sync::Arc<gpui_kit::RenderImage>,
        (subspace_lucide::IconName, AttachmentStatus),
    >,
    name: SharedString,
) -> Attachment {
    match face {
        Ok(image) => card
            .axis(gpui_kit::Axis::Vertical)
            .large()
            .size(rems(THUMB_REM))
            .media(AttachmentMedia::new().src(image)),
        Err((icon, status)) => card
            .small()
            .status(status)
            .media(AttachmentMedia::new().child(gpui_kit::component::Icon::from(icon)))
            .content(AttachmentContent::new().title(AttachmentTitle::new(name))),
    }
}

/// How far the focus ring sits outside its element, in pixels.
const RING_OUTSET: f32 = 3.;

/// The keyboard cursor's ring: drawn just outside its element, so showing
/// it never shifts the row.
pub fn focus_ring(tokens: &Tokens) -> Div {
    let outset = gpui_kit::px(-RING_OUTSET);
    div()
        .absolute()
        .top(outset)
        .left(outset)
        .right(outset)
        .bottom(outset)
        .rounded(tokens.control_radius)
        .border_2()
        .border_color(tokens.accent)
}

/// A reference that no longer opens (expired or deleted): says so and
/// keeps `key` visible. `ring` marks the keyboard cursor without moving it.
pub fn unavailable_chip(
    id: impl Into<ElementId>,
    key: impl Into<SharedString>,
    ring: bool,
    tokens: &Tokens,
) -> impl gpui_kit::IntoElement {
    let key = key.into();
    gpui_kit::component::h_flex()
        .id(id)
        .relative()
        .aria_label(SharedString::from(format!("Attachment unavailable: {key}")))
        .gap_1p5()
        .px_2()
        .py_0p5()
        .rounded(tokens.control_radius)
        .border_1()
        .border_color(tokens.line)
        .text_xs()
        .text_color(tokens.muted)
        .child(gpui_kit::component::Icon::from(subspace_lucide::IconName::FileX).xsmall())
        .child("Attachment unavailable")
        .child(key)
        .children(ring.then(|| focus_ring(tokens)))
        .test_support()
}

#[cfg(test)]
mod tests {
    #[test]
    fn sizes_read_in_decimal_bytes_kilobytes_and_megabytes() {
        assert_eq!(super::size_text(5), "5 bytes");
        assert_eq!(super::size_text(1500), "1.5 KB");
        assert_eq!(super::size_text(25_000_000), "25.0 MB");
        assert_eq!(super::size_text(999), "999 bytes");
    }
}
