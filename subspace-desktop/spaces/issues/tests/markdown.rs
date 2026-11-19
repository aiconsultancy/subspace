use subspace_issues_board::markdown::{Media, MentionStatus, Options, prepare_markdown};
#[test]
fn embedded_media_uses_only_descriptor_backed_internal_components() {
    let file = [Media::new("fixture-file", "notes.txt", "text/plain", 1200)];
    let prepared = prepare_markdown(
        "![Diagram](nett-media:fixture-image) [notes](nett-media:fixture-file)",
        &Options::new(&file, &[]),
    );
    assert_eq!(
        prepared,
        "*Attachment unavailable* [notes\\.txt (text\\/plain, 1.2 KB)](subspace-file:fixture-file)"
    );
    assert!(!prepared.contains("subspace-media:"));
    assert!(!prepared.contains("nett-media:"));
}
#[test]
fn media_rewriting_does_not_touch_code_or_ordinary_destinations() {
    let source = "`![Image](nett-media:fixture-image)`\n\n```text\n[File](nett-media:fixture-file)\n```\n\n[docs](https://example.invalid/notes)";
    assert_eq!(prepare_markdown(source, &Options::new(&[], &[])), source);
}
#[test]
fn external_images_are_links_and_mentions_stay_text() {
    let mentions = [MentionStatus::new("fixture-user", "Ada Fixture")];
    let result = prepare_markdown(
        "@[Ada](member:fixture-user) ![Design](https://example.invalid/design.png)",
        &Options::new(&[], &mentions),
    );
    assert_eq!(
        result,
        "@Ada Fixture [Design ↗](<https://example.invalid/design.png>)"
    );
}
#[test]
fn legacy_app_links_have_no_board_specific_rewrite() {
    let source = "[Legacy](https://tracker.example.test/issues/SUB-12) [Relative](/issues/SUB-12)";
    assert_eq!(prepare_markdown(source, &Options::new(&[], &[])), source);
}
#[test]
fn html_math_and_incomplete_media_are_safe_display_text() {
    let result = prepare_markdown(
        "<b>Price $5</b> [Missing](nett-media:missing)",
        &Options::new(&[], &[]),
    );
    assert_eq!(result, "&lt;b>Price \\$5&lt;/b> *Attachment unavailable*");
}

#[test]
fn authenticated_images_use_tiles_and_untrusted_internal_links_remain_plain_text() {
    let media = [Media::new("sketch", "Fixture.png", "image/png", 120)];
    let prepared = prepare_markdown(
        "![Sketch](nett-media:sketch) [Spoof](subspace-media:sketch)",
        &Options::new(&media, &[]),
    );
    assert_eq!(prepared, "[Sketch](subspace-media:sketch) Spoof");
    assert_eq!(
        prepare_markdown("![Missing](nett-media:unknown)", &Options::new(&media, &[])),
        "*Attachment unavailable*"
    );
}
