//! Temporary chat uploads. IRC owns its composer/cache; private transport stays in the API client.
pub(crate) mod cache;
mod intake;
pub(crate) mod outgoing;
pub(crate) mod session;
pub(crate) mod viewer;
use crate::hub::Hub;
use crate::transcript::RowContext;
use gpui_kit::component::h_flex;
use gpui_kit::{App, IntoElement as _, ParentElement as _, SharedString, Styled as _};
use subspace_api_client::chat_uploads::{self, Upload};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::media::{self, Thumb};

/// Whether `key` opens: its upload is known and not expired. Its sent text
/// then hides behind the thumbnail.
pub(crate) fn resolved<'a>(hub: &'a Hub, key: &str) -> Option<&'a Upload> {
    match hub.uploads.get(key) {
        Some(cache::Card::Ready(meta, _)) if !meta.expired() => Some(meta.as_ref()),
        _ => None,
    }
}

/// `text` without the sent form of every reference that resolves; a name
/// and size annotation hides only when it matches the upload.
pub(crate) fn shown_text(hub: Option<&Hub>, text: &str) -> String {
    match hub {
        Some(hub) => chat_uploads::without_references(text, |key| resolved(hub, key)),
        None => text.to_owned(),
    }
}

/// A message's attachments as one row of thumbnails, in sent order.
/// `focused` is the reference the transcript keyboard is on.
pub(crate) fn tiles(
    text: &str,
    context: &RowContext,
    focused: Option<&str>,
    tokens: &Tokens,
    cx: &App,
) -> Option<gpui_kit::AnyElement> {
    let keys = chat_uploads::references(text);
    if keys.is_empty() {
        return None;
    }
    let hub = context.hub.upgrade();
    let cards: Vec<_> = keys
        .into_iter()
        .map(|key| {
            let card = hub
                .as_ref()
                .and_then(|hub| hub.read(cx).uploads.get(&key).cloned());
            let ring = focused == Some(key.as_str());
            tile(key, card, ring, context, tokens)
        })
        .collect();
    Some(
        h_flex()
            .flex_wrap()
            .items_start()
            .gap_2()
            .children(cards)
            .into_any_element(),
    )
}

fn tile(
    key: String,
    card: Option<cache::Card>,
    ring: bool,
    context: &RowContext,
    tokens: &Tokens,
) -> gpui_kit::AnyElement {
    let id = SharedString::from(format!("chat-upload:{key}"));
    let (thumb, name, label, tooltip) = match card {
        Some(cache::Card::Ready(meta, image)) if !meta.expired() => {
            let thumb = match image {
                Some(image) => Thumb::Image(image),
                None if meta.content_type.starts_with("text/") => Thumb::Text,
                None => Thumb::File,
            };
            let (label, tooltip) = details(&meta);
            (thumb, meta.file_name.clone(), label, tooltip)
        }
        Some(cache::Card::Loading) => (
            Thumb::Loading,
            "Loading…".to_owned(),
            format!("Open attachment {key}, loading"),
            "Loading attachment…".to_owned(),
        ),
        Some(cache::Card::Failed) => (
            Thumb::Retry,
            "Couldn’t load".to_owned(),
            format!("Open attachment {key} to try again"),
            "Couldn’t load the attachment. Open it to try again.".to_owned(),
        ),
        None => (
            Thumb::File,
            "Sign in to open".to_owned(),
            format!("Attachment {key}. Sign in to Subspace to open it."),
            "Sign in to Subspace to open this attachment.".to_owned(),
        ),
        Some(_) => {
            return media::unavailable_chip(id, key, ring, tokens).into_any_element();
        }
    };
    let open = context.upload.clone();
    let open = move |window: &mut gpui_kit::Window, cx: &mut App| open(&key, window, cx);
    media::thumbnail(
        id,
        thumb,
        (name.into(), label.into(), tooltip.into()),
        (ring, open),
        tokens,
    )
    .into_any_element()
}

/// The thumbnail's accessible label, which names its action, and its
/// tooltip: name, size and expiry.
fn details(meta: &Upload) -> (String, String) {
    let size = media::size_text(meta.size_bytes);
    let expires = chrono::DateTime::parse_from_rfc3339(&meta.expires_at)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%-d %b %Y")
                .to_string()
        })
        .unwrap_or_else(|_| meta.expires_at.clone());
    (
        format!("Open {}, {size}, expires {expires}", meta.file_name),
        format!("{} · {size} · Expires {expires}", meta.file_name),
    )
}
