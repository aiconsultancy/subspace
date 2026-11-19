use super::{client, common};
use crate::chat_uploads::{self, MAX_BYTES, UploadDraft, UploadError};
use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

fn metadata(draft: &UploadDraft) -> Value {
    use sha2::{Digest as _, Sha256};
    let hash = Sha256::digest(draft.bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    json!({"id":draft.id(),"reference":draft.reference(),"fileName":draft.name(),"contentType":draft.mime(),
        "sizeBytes":draft.bytes().len(),"sha256":hash,"uploadedByUserId":"me",
        "createdAt":"2026-11-17T00:00:00Z","expiresAt":"2026-12-17T00:00:00Z","url":"https://untrusted.example.test/never-follow-this"})
}
#[test]
fn compact_keys_are_canonical_case_sensitive_and_bounded_in_irc_text() {
    let draft = UploadDraft::new(
        "message.md".into(),
        "text/markdown".into(),
        b"hello".to_vec(),
    )
    .unwrap();
    let key = draft.reference();
    assert_eq!(key.len(), 26);
    assert_eq!(chat_uploads::canonical(&key), Some(key.clone()));
    assert_eq!(
        chat_uploads::canonical(&format!("upl-{}", &key[4..])),
        Some(key.clone())
    );
    assert!(chat_uploads::canonical("UPL-AAAAAAAAAAAAAAAAAAAAAA").is_none());
    assert!(chat_uploads::canonical("UPL-AAAAAAAAAAAAAAAAAAAAAB").is_none());
    assert!(chat_uploads::references(&format!("x{key} {key}x")).is_empty());
    assert_eq!(chat_uploads::references(&format!("({key}),")), [key]);
}
#[test]
fn a_renewed_draft_keeps_the_file_under_a_new_canonical_key() {
    let draft = UploadDraft::new("a.txt".into(), "text/plain".into(), b"hi".to_vec()).unwrap();
    let renewed = draft.renewed();
    assert_ne!(renewed.id(), draft.id());
    assert_ne!(renewed.reference(), draft.reference());
    let key = renewed.reference();
    assert_eq!(chat_uploads::canonical(&key), Some(key));
    assert_eq!(
        (renewed.name(), renewed.mime(), renewed.bytes()),
        (draft.name(), draft.mime(), draft.bytes())
    );
}
#[test]
fn upload_drafts_admit_twenty_five_mb_and_refuse_oversized_or_unsupported_files() {
    assert!(
        UploadDraft::new(
            "exact.txt".into(),
            "text/plain".into(),
            vec![b'a'; MAX_BYTES]
        )
        .is_ok()
    );
    assert!(
        UploadDraft::new(
            "large.txt".into(),
            "text/plain".into(),
            vec![b'a'; MAX_BYTES + 1]
        )
        .is_err()
    );
    assert!(
        UploadDraft::new("script.svg".into(), "image/svg+xml".into(), b"svg".to_vec()).is_err()
    );
}
#[test]
fn documents_and_zip_keep_binary_bytes_and_download_with_their_mime_type() {
    for extension in ["zip", "pdf", "docx", "xlsx"] {
        let mime = chat_uploads::mime_for_extension(&extension.to_uppercase()).unwrap();
        let bytes = if extension == "pdf" {
            b"%PDF-1.7\n%\xff\xfe".to_vec()
        } else {
            b"PK\x03\x04\xff\x00\xfe".to_vec()
        };
        let draft =
            UploadDraft::new(format!("captain.{extension}"), mime.into(), bytes.clone()).unwrap();
        let upload = serde_json::from_value(metadata(&draft)).unwrap();
        let expected = bytes.clone();
        let mock = Mock::with_bytes(move |_| {
            (
                200,
                vec![
                    ("Content-Type".into(), mime.into()),
                    ("X-Content-Type-Options".into(), "nosniff".into()),
                ],
                expected.clone(),
            )
        });
        assert_eq!(client(&mock).chat_upload_bytes(&upload).unwrap(), bytes);
    }
    assert!(chat_uploads::mime_for_extension("doc").is_none());
    assert!(chat_uploads::mime_for_extension("xls").is_none());
    assert!(!chat_uploads::supported("application/msword"));
    assert!(!chat_uploads::supported("application/vnd.ms-excel"));
}
#[test]
fn create_is_single_attempt_and_manual_retry_keeps_id_and_bytes() {
    let draft =
        UploadDraft::new("brief.md".into(), "text/markdown".into(), b"brief".to_vec()).unwrap();
    let response = metadata(&draft).to_string();
    let mock = Mock::new(move |r| {
        if r.method == "POST" {
            (201, response.clone())
        } else {
            common(r)
        }
    });
    let api = client(&mock);
    assert!(api.create_chat_upload(&draft).is_ok());
    assert!(api.create_chat_upload(&draft).is_ok());
    let bodies: Vec<_> = mock
        .requests()
        .into_iter()
        .filter(|r| r.method == "POST")
        .map(|r| r.body)
        .collect();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], bodies[1]);
    assert_eq!(
        serde_json::from_str::<Value>(&bodies[0]).unwrap()["uploadId"],
        draft.id()
    );
}
#[test]
fn upload_content_stays_on_fixed_origin_and_checks_integrity() {
    let draft =
        UploadDraft::new("brief.txt".into(), "text/plain".into(), b"brief".to_vec()).unwrap();
    let upload = serde_json::from_value(metadata(&draft)).unwrap();
    let mock = Mock::with_bytes(|_| {
        (
            200,
            vec![
                ("Content-Type".into(), "text/plain".into()),
                ("X-Content-Type-Options".into(), "nosniff".into()),
            ],
            b"brief".to_vec(),
        )
    });
    assert_eq!(client(&mock).chat_upload_bytes(&upload).unwrap(), b"brief");
    assert_eq!(
        mock.requests()[0].path,
        format!("/api/v1/chat/uploads/{}/content", draft.reference())
    );
    let tampered = Mock::with_bytes(|_| {
        (
            200,
            vec![
                ("Content-Type".into(), "text/plain".into()),
                ("X-Content-Type-Options".into(), "nosniff".into()),
            ],
            b"wrong".to_vec(),
        )
    });
    assert_eq!(
        client(&tampered).chat_upload_bytes(&upload),
        Err(UploadError::Unavailable)
    );
}

#[test]
fn names_match_the_apis_path_control_and_format_character_normalisation() {
    let draft = UploadDraft::new(
        "folder\\  résumé\u{202e}.md\n  ".into(),
        "text/markdown".into(),
        b"brief".to_vec(),
    )
    .unwrap();
    assert_eq!(draft.name(), "résumé.md");
    assert!(UploadDraft::new("x".repeat(201), "text/plain".into(), b"brief".to_vec()).is_err());
    assert!(UploadDraft::new(" \u{202e}".into(), "text/plain".into(), b"brief".to_vec()).is_err());
}

#[test]
fn irc_previews_bound_utf8_bytes_and_remain_a_single_short_line() {
    let draft =
        UploadDraft::new("brief.md".into(), "text/markdown".into(), b"brief".to_vec()).unwrap();
    let upload = serde_json::from_value(metadata(&draft)).unwrap();
    let message = chat_uploads::message(&upload, &format!("{}\nsecond line", "🐱".repeat(200)));
    assert!(message.starts_with(&draft.reference()));
    assert!(message.len() < 200);
    assert_eq!(message.lines().count(), 1);
    assert!(!message.contains("second line"));
}

fn uploaded(name: &str, bytes: &[u8]) -> chat_uploads::Upload {
    let draft = UploadDraft::new(name.into(), "text/plain".into(), bytes.to_vec()).unwrap();
    serde_json::from_value(metadata(&draft)).unwrap()
}

#[test]
fn reference_text_names_the_file_and_size_and_shortens_only_the_name() {
    let upload = uploaded("notes.txt", b"hello");
    let text = chat_uploads::reference_text(&upload, 400);
    assert_eq!(text, format!("{} notes.txt (5 bytes)", upload.reference));
    let long = uploaded(&format!("{}.txt", "n".repeat(190)), b"hello");
    let short = chat_uploads::reference_text(&long, 60);
    assert!(short.len() <= 60);
    assert!(short.starts_with(&long.reference));
    assert!(short.ends_with(" (5 bytes)"));
    let tight = chat_uploads::reference_text(&long, 10);
    assert_eq!(tight, format!("{} (5 bytes)", long.reference));
}

#[test]
fn lines_use_one_line_when_it_fits_and_never_split_a_reference() {
    let refs: Vec<String> = (0..3)
        .map(|n| chat_uploads::reference_text(&uploaded(&format!("f{n}.txt"), b"x"), 400))
        .collect();
    let one = chat_uploads::lines("hello", &refs, 400);
    assert_eq!(one, vec![format!("hello {}", refs.join(" "))]);
    assert_eq!(
        chat_uploads::lines("", &refs[..1], 400),
        vec![refs[0].clone()]
    );
    assert_eq!(
        chat_uploads::lines("only text", &[], 400),
        vec!["only text"]
    );
    assert!(chat_uploads::lines("  ", &[], 400).is_empty());
    let budget = refs[0].len() * 2 + 1;
    let packed = chat_uploads::lines("hello", &refs, budget);
    assert_eq!(
        packed,
        vec![
            "hello".to_owned(),
            format!("{} {}", refs[0], refs[1]),
            refs[2].clone()
        ]
    );
    let multi = chat_uploads::lines("a\nb", &refs[..1], 400);
    assert_eq!(multi, vec!["a\nb".to_owned(), refs[0].clone()]);
    let order: Vec<String> = chat_uploads::lines("t", &refs, 10)
        .iter()
        .flat_map(|line| chat_uploads::references(line))
        .collect();
    let keys: Vec<String> = refs
        .iter()
        .flat_map(|r| chat_uploads::references(r))
        .collect();
    assert_eq!(order, keys);
}

#[test]
fn reference_spans_cover_new_and_legacy_forms_and_resolved_text_can_be_hidden() {
    let a = uploaded("a.png", b"aa");
    let b = uploaded("b.md", b"bbb");
    let text = format!(
        "look {} \u{2014} a.png (2 bytes) and {} b.md (3 bytes) done",
        a.reference, b.reference
    );
    let spans = chat_uploads::reference_spans(&text);
    assert_eq!(spans.len(), 2);
    assert_eq!(
        &text[spans[0].1.clone()],
        format!("{} \u{2014} a.png (2 bytes)", a.reference)
    );
    assert_eq!(
        &text[spans[1].1.clone()],
        format!("{} b.md (3 bytes)", b.reference)
    );
    let both = |key: &str| [&a, &b].into_iter().find(|u| u.reference == key);
    assert_eq!(
        chat_uploads::without_references(&text, both),
        "look and done"
    );
    let only_a = |key: &str| (key == a.reference).then_some(&a);
    assert_eq!(
        chat_uploads::without_references(&text, only_a),
        format!("look and {} b.md (3 bytes) done", b.reference)
    );
    let bare = format!("{} {}", a.reference, b.reference);
    assert_eq!(chat_uploads::without_references(&bare, both), "");
    let spans = chat_uploads::reference_spans(&bare);
    assert_eq!(spans[0].1, 0..26);
}

#[test]
fn prose_with_a_byte_count_after_a_key_stays_visible() {
    let a = uploaded("a.png", b"aa");
    let resolve = |key: &str| (key == a.reference).then_some(&a);
    let wrong_size = format!("{} a.png (9 bytes)", a.reference);
    assert_eq!(
        chat_uploads::without_references(&wrong_size, resolve),
        "a.png (9 bytes)"
    );
    let prose = format!(
        "{} anything at all, the log was (2 bytes) long",
        a.reference
    );
    assert_eq!(
        chat_uploads::without_references(&prose, resolve),
        "anything at all, the log was (2 bytes) long"
    );
    let other_name = format!("see {} b.png (2 bytes) ok", a.reference);
    assert_eq!(
        chat_uploads::without_references(&other_name, resolve),
        "see b.png (2 bytes) ok"
    );
    let shortened = format!("{} a.p (2 bytes) ok", a.reference);
    assert_eq!(chat_uploads::without_references(&shortened, resolve), "ok");
    let unnamed = format!("{} (2 bytes)", a.reference);
    assert_eq!(chat_uploads::without_references(&unnamed, resolve), "");
}
