use anyhow::{Result, ensure};

/// A portable, explicit source allowlist. Hidden files and generated/private
/// directories cannot become source merely by being listed in a definition.
pub fn validate_source_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && path.len() <= 240,
        "source path must be 1 to 240 bytes"
    );
    let segments: Vec<_> = path.split('/').collect();
    ensure!(
        segments.iter().all(|segment| safe_segment(segment)),
        "source path must be relative and must not name hidden, private or generated files"
    );
    ensure!(
        is_source_file(path),
        "source file extension is not shareable"
    );
    ensure!(
        path != "dashboard.json",
        "dashboard.json is reserved for the definition"
    );
    Ok(())
}

fn safe_segment(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.starts_with('.')
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && !matches!(
            segment.to_ascii_lowercase().as_str(),
            "node_modules"
                | "__pycache__"
                | "venv"
                | "logs"
                | "results"
                | "snapshots"
                | "private"
                | "settings"
                | "target"
                | "dist"
        )
}

fn is_source_file(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, extension)| {
        matches!(
            extension,
            "py" | "js"
                | "mjs"
                | "cjs"
                | "ts"
                | "json"
                | "toml"
                | "lock"
                | "yaml"
                | "yml"
                | "md"
                | "txt"
        )
    })
}
