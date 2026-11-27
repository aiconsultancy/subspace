use super::{client, media::descriptor};
use crate::uploads::{ImageDeclaration, UploadError};
use serde_json::{Value, json};
mod intake;
mod lifecycle;
mod names;
mod recovery;
use subspace_framework::client_test_support::Mock;

fn declaration() -> ImageDeclaration {
    ImageDeclaration::new("Fixture.txt", "text/plain", b"fixture").unwrap()
}
fn saved() -> Value {
    let mut media = descriptor("uploaded-1");
    media["kind"] = json!("file");
    media["fileName"] = json!("Fixture.txt");
    media["contentType"] = json!("text/plain");
    media["sizeBytes"] = json!(7);
    media["sha256"] = json!(declaration().sha256);
    media
}

#[test]
fn multipart_sends_one_file_and_unclear_uploads_are_never_retried() {
    let mock = Mock::new(|_| (201, saved().to_string()));
    let result = client(&mock)
        .upload_attachment("SUB-1", &declaration(), b"fixture")
        .unwrap();
    assert_eq!(result.media_id, "uploaded-1");
    let request = &mock.requests()[0];
    assert_eq!(request.path, "/api/v1/issues/SUB-1/attachments");
    assert!(request.body.contains(
        "name=\"file\"; filename=\"Fixture.txt\"\r\nContent-Type: text/plain\r\n\r\nfixture\r\n"
    ));
    assert!(
        !request
            .headers
            .iter()
            .any(|(key, _)| key == "authorization" || key == "idempotency-key")
    );
    let mock = Mock::with_headers(|_| (503, vec![("Retry-After".into(), "1".into())], "{}".into()));
    assert!(matches!(
        client(&mock).upload_attachment("SUB-1", &declaration(), b"fixture"),
        Err(UploadError::Unclear { retry_after: 1 })
    ));
    assert_eq!(mock.requests().len(), 1);
}

#[test]
fn image_and_file_removals_use_their_distinct_contract_routes() {
    let mock = Mock::new(|_| (200, "true".into()));
    let mut media = serde_json::from_value(saved()).unwrap();
    assert!(client(&mock).delete_media(&media).is_ok());
    media.kind = "image".into();
    assert!(client(&mock).delete_media(&media).is_ok());
    let requests = mock.requests();
    assert_eq!(
        (&*requests[0].method, &*requests[0].path),
        ("DELETE", "/api/v1/issues/attachments/uploaded-1")
    );
    assert_eq!(
        (&*requests[1].method, &*requests[1].path),
        ("POST", "/api/v1/issues/media/uploaded-1/delete")
    );
}

#[test]
fn adding_a_raster_as_a_file_posts_to_attachments_not_inline_images() {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(2, 1)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let bytes = bytes.into_inner();
    let declaration = ImageDeclaration::new("Fixture.png", "image/png", &bytes).unwrap();
    let mut file = descriptor("photo");
    file["kind"] = json!("file");
    file["sizeBytes"] = json!(bytes.len());
    file["sha256"] = json!(declaration.sha256);
    let mock = Mock::new(move |_| (201, file.to_string()));
    let attempt = client(&mock).upload_issue_attachment("SUB-1", "any-key", &declaration, &bytes);
    assert!(attempt.result.unwrap().can_save_attachment());
    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        (&*requests[0].method, &*requests[0].path),
        ("POST", "/api/v1/issues/SUB-1/attachments")
    );
}

#[test]
fn preflight_checks_raster_dimensions_mime_names_and_document_limits() {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(2, 1)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    assert!(ImageDeclaration::new("Fixture.png", "image/png", bytes.get_ref()).is_ok());
    assert!(ImageDeclaration::new("Fixture.jpg", "image/jpeg", bytes.get_ref()).is_err());
    let mut wide = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(4097, 1)
        .write_to(&mut wide, image::ImageFormat::Png)
        .unwrap();
    assert!(ImageDeclaration::new("Fixture.png", "image/png", wide.get_ref()).is_err());
    assert!(ImageDeclaration::new("Fixture.txt", "text/plain", &[b'x'; 1]).is_ok());
    assert!(ImageDeclaration::new("Fixture.txt", "text/plain", &[0]).is_err());
    assert!(ImageDeclaration::new("../Fixture.txt", "text/plain", b"fixture").is_err());
    assert!(ImageDeclaration::new("Fixture.svg", "image/svg+xml", b"<svg/>").is_err());
    assert!(ImageDeclaration::new("Fixture.txt", "text/plain", &vec![b'x'; 10_000_001]).is_err());
}
