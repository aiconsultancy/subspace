use super::super::intake::{self, Preview};
use std::sync::Arc;
use subspace_api_client::chat_uploads::{Upload, UploadError};

pub(super) fn read(
    key: &str,
    scope: Option<&super::super::session::Scope>,
) -> Result<(Upload, Option<Arc<[u8]>>, Preview), UploadError> {
    let scope = scope.ok_or(UploadError::Access)?;
    let client = scope.client()?;
    let meta = client.chat_upload(key)?;
    if intake::is_file(&meta.content_type) {
        scope.client()?;
        return Ok((meta, None, Preview::File));
    }
    let bytes = client.chat_upload_bytes(&meta)?;
    let content = intake::preview(&bytes, &meta.content_type)?;
    scope.client()?;
    Ok((meta, Some(bytes.into()), content))
}
