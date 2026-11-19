use std::path::Path;

use serde_json::{Value, json};

use super::settings::{BODY, HEIGHT, MAX_HEIGHT, MONO, SCALE, WIDTH, selected_minimum_height};

pub(crate) fn write_manifest(output: &Path, records: Vec<Value>) {
    let revision = std::env::var("SUBSPACE_GALLERY_SOURCE_REVISION")
        .expect("record the captured source revision explicitly");
    assert!(revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let manifest = json!({
        "sourceRevision": revision,
        "platform": "linux",
        "context": "GPUI TestPlatform / HeadlessAppContext",
        "renderer": "current_headless_renderer / WgpuHeadlessRenderer",
        "bodyFont": BODY,
        "monoFont": MONO,
        "width": WIDTH,
        "height": HEIGHT,
        "maxHeight": MAX_HEIGHT,
        "visualFixtureVersion": "full-gallery-v1",
        "minimumHeight": selected_minimum_height(),
        "scaleFactor": SCALE,
        "graphicsIcd": std::env::var("VK_ICD_FILENAMES").ok(),
        "cards": records,
        "limits": "Fixture cards at rest only; per-card height grows to fit natural card bounds. Use each card's width, height and scaleFactor for web comparison. Internal sample overflow is not measured. No installed app, real configuration, native window chrome, pointer/keyboard workflow or cross-platform proof.",
    });
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("serialise proof manifest"),
    )
    .expect("write proof manifest");
}
