use super::UploadDraft;
use base64::{display::Base64Display, engine::general_purpose::STANDARD};
use serde::{Serialize, Serializer};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Body<'a> {
    upload_id: &'a str,
    file_name: &'a str,
    content_type: &'a str,
    base64: Encoded<'a>,
}
struct Encoded<'a>(&'a [u8]);
impl Serialize for Encoded<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&Base64Display::new(self.0, &STANDARD))
    }
}
pub(super) fn create(draft: &UploadDraft) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&Body {
        upload_id: draft.id(),
        file_name: draft.name(),
        content_type: draft.mime(),
        base64: Encoded(draft.bytes()),
    })
}
