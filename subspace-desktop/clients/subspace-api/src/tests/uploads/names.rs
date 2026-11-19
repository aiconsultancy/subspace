use super::saved;
use crate::{tests::client, uploads::ImageDeclaration};
use serde_json::json;
use subspace_framework::client_test_support::Mock;

#[test]
fn filenames_normalize_format_characters_and_whitespace_before_upload() {
    for (source, expected) in [
        ("report .pdf", "report .pdf"),
        (" \u{200e}report.pdf\u{200e} ", "report.pdf"),
    ] {
        let declaration =
            ImageDeclaration::new(source, "application/pdf", b"%PDF-fixture").unwrap();
        assert_eq!(declaration.file_name, expected);
        let mut media = saved();
        media["fileName"] = json!(expected);
        media["contentType"] = json!("application/octet-stream");
        media["sizeBytes"] = json!(declaration.size_bytes);
        media["sha256"] = json!(declaration.sha256.to_uppercase());
        let mock = Mock::new(move |_| (201, media.to_string()));
        assert!(
            client(&mock)
                .upload_attachment("1", &declaration, b"%PDF-fixture")
                .is_ok()
        );
        assert!(
            mock.requests()[0]
                .body
                .contains(&format!("filename=\"{expected}\""))
        );
    }
}

#[test]
fn filenames_use_utf16_limits_and_recovery_requires_name_hash_and_size() {
    let exact = format!("{}.txt", "🙂".repeat(98));
    assert!(ImageDeclaration::new(&exact, "text/plain", b"fixture").is_ok());
    assert!(ImageDeclaration::new(&format!("x{exact}"), "text/plain", b"fixture").is_err());
    assert!(
        ImageDeclaration::new(
            &format!("{}.txt", "a".repeat(197)),
            "text/plain",
            b"fixture"
        )
        .is_err()
    );
    let declaration = ImageDeclaration::new("Fixture.txt", "text/plain", b"fixture").unwrap();
    let mut media = saved();
    media["fileName"] = json!(" \u{200e}Fixture.txt ");
    let matching = serde_json::from_value(media.clone()).unwrap();
    assert!(declaration.matches(&matching));
    for (key, value) in [
        ("fileName", json!("Other.txt")),
        ("sizeBytes", json!(8)),
        ("sha256", json!("different")),
    ] {
        let mut wrong = media.clone();
        wrong[key] = value;
        assert!(!declaration.matches(&serde_json::from_value(wrong).unwrap()));
    }
}
