use chrono::{TimeZone as _, Utc};

use crate::log::{self, Level, Logger, Record};

use super::{
    BuildIdentity, ErrorDetails, LaunchKind, LocalDiagnostics, RedactionContext, RuntimeDetails,
};

const TARGET: &str = "diagnostics-logging-tests";
const ENV_VALUE: &str = "TEST_ONLY_LOG_ENV_VALUE";
const FULL_PATH: &str = "/test-only/one:/test-only/two";
const SAFE_PAYLOAD: &str = "Read ~/cache [redacted] [redacted] [redacted] [redacted] [redacted]";
const MAX_VALUE_CHARS: usize = 4_096;

fn payload() -> String {
    format!(
        "Read /Users/fixture/cache {ENV_VALUE} target-private https://test-user:test-proxy-password@proxy.invalid CUSTOM_ENV='TEST_ONLY quoted environment value' {FULL_PATH}"
    )
}

fn snapshot() -> LocalDiagnostics {
    LocalDiagnostics::new(
        BuildIdentity::new("subspace", "1.2.3")
            .with_build("beta", "abc123")
            .with_platform("macos", "aarch64"),
        "1.16.0",
    )
    .with_os_version("15.2")
    .with_runtime(RuntimeDetails::new(
        "/Users/fixture/Apps/Test App",
        LaunchKind::Desktop,
        "login shell /bin/zsh",
        None,
    ))
    .with_redaction(RedactionContext::new(
        Some("/Users/fixture".into()),
        [
            ENV_VALUE.into(),
            FULL_PATH.into(),
            "fixture".into(),
            "1".into(),
        ],
    ))
}

fn checkpoint() -> u64 {
    log::recent().last().map_or(0, |record| record.seq)
}

fn records_after(surface: &str, seq: u64) -> Vec<Record> {
    log::recent_after(seq)
        .into_iter()
        .filter(|record| {
            record.message.contains(surface)
                || record
                    .fields
                    .iter()
                    .any(|(_, value)| value.contains(surface))
        })
        .collect()
}

fn private_error() -> ErrorDetails {
    let payload = payload();
    ErrorDetails::new(
        format!("Title {payload}"),
        format!("Message {payload}"),
        format!("Stage {payload}"),
    )
    .with_code(format!("Code {payload}"))
    .with_chain(format!("Chain {payload}\nUnderlying error."))
    .with_private_values(["target-private".into()])
    .with_recorded_at(Utc.with_ymd_and_hms(2026, 11, 9, 14, 0, 0).unwrap())
}

#[test]
fn logging_and_copy_share_sanitized_error_fields_and_the_exact_details_block() {
    let snapshot = snapshot();
    let error = private_error();
    let surface = format!("diagnostics-logging-privacy {}", payload());
    let seq = checkpoint();
    snapshot.log_error(&Logger::new(TARGET), &surface, &error);
    let records = records_after("diagnostics-logging-privacy", seq);
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.level, Level::Error);
    assert_eq!(record.message, format!("Title {SAFE_PAYLOAD}"));
    let copied = snapshot.local_copy(Some(&error));
    let expected = [
        (
            "surface",
            format!("diagnostics-logging-privacy {SAFE_PAYLOAD}"),
        ),
        ("title", format!("Title {SAFE_PAYLOAD}")),
        ("message", format!("Message {SAFE_PAYLOAD}")),
        ("code", format!("Code {SAFE_PAYLOAD}")),
        ("stage", format!("Stage {SAFE_PAYLOAD}")),
        ("occurred_at", "2026-11-09T14:00:00Z".into()),
        (
            "error_chain",
            format!("Chain {SAFE_PAYLOAD} Underlying error."),
        ),
        ("details", copied.markdown().into()),
    ];
    assert_eq!(
        record.fields,
        expected
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<Vec<_>>()
    );
    let serialized = serde_json::to_string(record).unwrap();
    for forbidden in [
        ENV_VALUE,
        "target-private",
        "proxy.invalid",
        "test-proxy-password",
        "TEST_ONLY quoted environment value",
        "/Users/fixture",
        "/test-only/one",
        "/test-only/two",
    ] {
        assert!(!serialized.contains(forbidden), "{serialized}");
    }
}

#[test]
fn two_accepted_calls_emit_two_records_but_copying_is_silent() {
    let snapshot = LocalDiagnostics::default();
    let surface = "diagnostics-logging-occurrences";
    let error = ErrorDetails::new(
        format!("Operation failed {surface}"),
        "Retry the operation.",
        "check",
    )
    .with_observed_at(Utc.with_ymd_and_hms(2026, 11, 9, 14, 1, 0).unwrap());
    let seq = checkpoint();
    let first = snapshot.local_copy(Some(&error));
    let second = snapshot.local_copy(Some(&error));
    assert_eq!(first.markdown(), second.markdown());
    assert!(records_after(surface, seq).is_empty());
    let logger = Logger::new(TARGET);
    snapshot.log_error(&logger, surface, &error);
    snapshot.log_error(&logger, surface, &error);
    let records = records_after(surface, seq);
    assert_eq!(records.len(), 2);
    assert!(records[0].seq < records[1].seq);
    for record in records {
        assert_eq!(
            record
                .fields
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>(),
            vec![
                "surface",
                "title",
                "message",
                "stage",
                "observed_at",
                "details"
            ]
        );
        assert!(
            record
                .fields
                .contains(&("observed_at".into(), "2026-11-09T14:01:00Z".into()))
        );
        assert!(
            record
                .fields
                .contains(&("details".into(), first.markdown().into()))
        );
        assert!(!first.markdown().contains("- Error chain:"));
    }
}

#[test]
fn chain_is_readable_and_bounded_identically_in_copy_and_log() {
    let snapshot = LocalDiagnostics::default();
    let surface = "diagnostics-logging-chain";
    let error = ErrorDetails::new("Operation failed", "Retry the operation.", "check")
        .with_chain(format!("Readable cause\n{}", "é".repeat(5_000)));
    let seq = checkpoint();
    snapshot.log_error(&Logger::new(TARGET), surface, &error);
    let records = records_after(surface, seq);
    assert_eq!(records.len(), 1);
    let chain = &records[0]
        .fields
        .iter()
        .find(|(key, _)| key == "error_chain")
        .unwrap()
        .1;
    assert!(chain.starts_with("Readable cause "));
    assert!(chain.ends_with(" [truncated]"));
    assert_eq!(
        chain.chars().count(),
        MAX_VALUE_CHARS + " [truncated]".chars().count()
    );
    assert!(
        snapshot
            .local_copy(Some(&error))
            .markdown()
            .contains(&format!("- Error chain: {chain}"))
    );
}
