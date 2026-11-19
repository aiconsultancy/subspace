//! Bounded, compile-time help assets. This interface never reads files or URLs.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_IMAGES: usize = 3;
const MAX_BYTES: usize = 512 * 1024;
const MAX_ENCODED: usize = MAX_BYTES.div_ceil(3) * 4;
const MAX_ID_BYTES: usize = 64;
const MAX_CAPTION_BYTES: usize = 512;
const WEBP_FORMAT_OFFSET: usize = 8;
const WEBP_HEADER_BYTES: usize = 12;

/// A board-owned image compiled into the binary with `include_bytes!`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HelpImage {
    pub id: &'static str,
    pub caption: &'static str,
    pub media_type: &'static str,
    pub bytes: &'static [u8],
}

impl HelpImage {
    pub const fn new(
        id: &'static str,
        caption: &'static str,
        media_type: &'static str,
        bytes: &'static [u8],
    ) -> Self {
        Self {
            id,
            caption,
            media_type,
            bytes,
        }
    }

    /// The selected image's typed transport reply, never part of the guide text.
    pub fn reply(&self, board_id: &str) -> Result<Value, String> {
        validate_images(&[*self])?;
        super::encoded(HelpImageReply {
            kind: ReplyKind::SubspaceBundledHelpImage,
            board_id: board_id.into(),
            image_id: self.id.into(),
            caption: self.caption.into(),
            media_type: self.media_type.into(),
            data: base64::engine::general_purpose::STANDARD.encode(self.bytes),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReplyKind {
    SubspaceBundledHelpImage,
}

/// Validated image reply shared by the SDK and MCP adapters.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct HelpImageReply {
    kind: ReplyKind,
    pub board_id: String,
    pub image_id: String,
    pub caption: String,
    pub media_type: String,
    pub data: String,
}

/// Decode only the reserved help tool's typed envelope. Ordinary JSON stays text.
pub fn help_image_reply(tool_name: &str, value: &Value) -> Option<HelpImageReply> {
    if tool_name != super::HELP_TOOL || value.get("data")?.as_str()?.len() > MAX_ENCODED {
        return None;
    }
    let reply: HelpImageReply = serde_json::from_value(value.clone()).ok()?;
    valid_reply(reply)
}

fn valid_reply(reply: HelpImageReply) -> Option<HelpImageReply> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&reply.data)
        .ok()?;
    (valid_id(&reply.board_id)
        && valid_id(&reply.image_id)
        && valid_caption(&reply.caption)
        && valid_bytes(&reply.media_type, &bytes))
    .then_some(reply)
}

/// Reject invalid catalogs as a whole, rather than silently exposing a subset.
pub fn validate_images(images: &[HelpImage]) -> Result<(), String> {
    if images.len() > MAX_IMAGES {
        return Err("A board may bundle at most 3 help images.".into());
    }
    for (index, image) in images.iter().enumerate() {
        if !valid_id(image.id)
            || !valid_caption(image.caption)
            || !valid_bytes(image.media_type, image.bytes)
            || images[..index].iter().any(|earlier| earlier.id == image.id)
        {
            return Err(format!("Invalid bundled help image: {}.", image.id));
        }
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_caption(caption: &str) -> bool {
    !caption.trim().is_empty() && caption.len() <= MAX_CAPTION_BYTES
}

fn valid_bytes(media_type: &str, bytes: &[u8]) -> bool {
    if bytes.len() > MAX_BYTES {
        return false;
    }
    match media_type {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/webp" => {
            bytes.starts_with(b"RIFF")
                && bytes.get(WEBP_FORMAT_OFFSET..WEBP_HEADER_BYTES) == Some(b"WEBP")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{HelpImage, help_image_reply};

    #[test]
    fn image_replies_require_the_reserved_tool_exact_tag_and_valid_bounded_raster() {
        let image = HelpImage::new("dashboard", "Test", "image/png", b"\x89PNG\r\n\x1a\ntest");
        let mut value = image.reply("lab").unwrap();
        assert!(help_image_reply("board_help", &value).is_some());
        assert!(help_image_reply("ordinary_read", &value).is_none());
        value["kind"] = "arbitrary_image".into();
        assert!(help_image_reply("board_help", &value).is_none());
        value["kind"] = "subspace_bundled_help_image".into();
        value["media_type"] = "image/svg+xml".into();
        assert!(help_image_reply("board_help", &value).is_none());
        value["media_type"] = "image/png".into();
        value["data"] = "x".repeat(700_000).into();
        assert!(help_image_reply("board_help", &value).is_none());
    }
}
