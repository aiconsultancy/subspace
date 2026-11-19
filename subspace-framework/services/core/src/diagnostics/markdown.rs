use super::{ErrorDetails, LocalDiagnostics, RedactionContext, RuntimeDetails, error::ErrorTime};

pub(super) fn render(
    snapshot: &LocalDiagnostics,
    error: Option<&ErrorDetails>,
    diagnostics: String,
) -> String {
    let mut sections = Vec::new();
    if let Some(error) = error {
        sections.push(error_section(snapshot, error));
    }
    sections.push(diagnostics);
    sections.join("\n\n")
}

pub(super) fn section(title: &str, fields: &[(&str, String)]) -> String {
    let lines: Vec<_> = fields
        .iter()
        .map(|(name, value)| format!("- {name}: {value}"))
        .collect();
    format!("## {title}\n\n{}", lines.join("\n"))
}

fn error_section(snapshot: &LocalDiagnostics, error: &ErrorDetails) -> String {
    let fields = error_fields(snapshot, error)
        .into_iter()
        .map(|(name, _, value)| (name, value))
        .collect::<Vec<_>>();
    section("Error details", &fields)
}

/// One sanitized fact projection with both display labels and structured keys.
pub(super) fn error_fields(
    snapshot: &LocalDiagnostics,
    error: &ErrorDetails,
) -> Vec<(&'static str, &'static str, String)> {
    let redaction = &snapshot.redaction;
    let text = |value: &str| redaction.error_text(value, &error.private_values);
    let mut fields = vec![
        ("Title", "title", text(&error.title)),
        ("Message", "message", text(&error.message)),
    ];
    if let Some(code) = &error.code {
        fields.push(("Code", "code", text(code)));
    }
    fields.push(("Stage", "stage", text(&error.stage)));
    fields.push(time_field(&error.time));
    if let Some(chain) = &error.chain {
        fields.push(("Error chain", "error_chain", text(chain)));
    }
    fields
}

fn time_field(time: &ErrorTime) -> (&'static str, &'static str, String) {
    match time {
        ErrorTime::Recorded(at) => (
            "Occurred at (UTC)",
            "occurred_at",
            at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ),
        ErrorTime::Observed(at) => (
            "Observed at (UTC)",
            "observed_at",
            at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ),
    }
}

pub(super) fn runtime_fields(
    runtime: Option<&RuntimeDetails>,
    redaction: &RedactionContext,
) -> Vec<(&'static str, String)> {
    vec![
        (
            "Executable",
            redaction.path_field(
                runtime
                    .map(|runtime| runtime.executable.as_str())
                    .unwrap_or("unavailable"),
            ),
        ),
        (
            "Launch",
            runtime
                .map(|runtime| runtime.launch.label())
                .unwrap_or("unknown")
                .into(),
        ),
        (
            "PATH source",
            redaction.source(
                runtime
                    .map(|runtime| runtime.path_source.as_str())
                    .unwrap_or("unavailable"),
            ),
        ),
        (
            "Azure CLI",
            runtime
                .and_then(|runtime| runtime.az_path.as_deref())
                .map(|path| redaction.path_field(path))
                .unwrap_or_else(|| "not found on the app's PATH".into()),
        ),
    ]
}
