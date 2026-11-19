use gpui_kit::RenderImage;
use std::io::Read as _;
use std::sync::Arc;
use subspace_api_client::chat_uploads::{MAX_BYTES, UploadDraft, UploadError};
use subspace_framework::dashboard::chat_input::attachments::AttachmentSource;
#[derive(Clone)]
pub(crate) enum Preview {
    Text(String),
    Image(Arc<RenderImage>),
    File,
}
pub(crate) fn prepare(source: AttachmentSource) -> Result<(UploadDraft, Preview), UploadError> {
    let (name, mime, bytes) = match source {
        AttachmentSource::ClipboardImage {
            bytes,
            mime_type,
            display_name,
        } => (display_name, mime_type.to_owned(), bytes.to_vec()),
        AttachmentSource::Path(path) => {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or(UploadError::Invalid)?
                .to_owned();
            let mime = subspace_api_client::chat_uploads::mime_for_extension(
                path.extension().and_then(|e| e.to_str()).unwrap_or(""),
            )
            .ok_or(UploadError::Invalid)?;
            let file = std::fs::File::open(path).map_err(|_| UploadError::Invalid)?;
            let mut bytes = Vec::new();
            file.take(MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| UploadError::Invalid)?;
            (name, mime.to_owned(), bytes)
        }
    };
    let draft = UploadDraft::new(name, mime.clone(), bytes)?;
    let preview = preview(draft.bytes(), &mime)?;
    Ok((draft, preview))
}
pub(crate) fn is_file(mime: &str) -> bool {
    mime.starts_with("application/")
}
pub(crate) fn preview(bytes: &[u8], mime: &str) -> Result<Preview, UploadError> {
    if mime.starts_with("image/") {
        Ok(Preview::Image(
            subspace_framework::dashboard::images::decode_with_limits(
                bytes,
                mime,
                super::cache::limits(),
            )
            .map_err(|_| UploadError::InvalidImage)?,
        ))
    } else if is_file(mime) {
        Ok(Preview::File)
    } else {
        Ok(Preview::Text(
            String::from_utf8(bytes.to_vec()).map_err(|_| UploadError::Invalid)?,
        ))
    }
}
pub(crate) fn preview_text(text: &str) -> (&str, bool) {
    const PREVIEW_BYTES: usize = 1024 * 1024;
    let mut end = text.len().min(PREVIEW_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], end < text.len())
}
