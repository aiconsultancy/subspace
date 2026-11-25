//! The file route's optional binary page. Decode in the caller's background
//! read, and never include either representation in diagnostics.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, de};

const MAX_PAGE_BYTES: usize = 1024 * 1024;
const BASE64_INPUT_GROUP: usize = 3;
const BASE64_OUTPUT_GROUP: usize = 4;

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum DiffSide {
    Old,
    New,
}

impl DiffSide {
    pub(super) fn query(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::New => "new",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DiffContent {
    pub side: DiffSide,
    #[serde(rename = "encoding")]
    _encoding: Encoding,
    pub offset: u64,
    pub size: u64,
    #[serde(rename = "data", deserialize_with = "decode")]
    pub bytes: Vec<u8>,
    pub next_page: Option<String>,
}

#[derive(Deserialize)]
enum Encoding {
    #[serde(rename = "base64")]
    Base64,
}

fn decode<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let data = String::deserialize(deserializer)?;
    if data.len() > MAX_PAGE_BYTES.div_ceil(BASE64_INPUT_GROUP) * BASE64_OUTPUT_GROUP {
        return Err(de::Error::custom("Binary diff page exceeds its size limit"));
    }
    STANDARD
        .decode(data)
        .ok()
        .filter(|bytes| bytes.len() <= MAX_PAGE_BYTES)
        .ok_or_else(|| de::Error::custom("Invalid binary diff page"))
}
