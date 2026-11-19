use chrono::{TimeZone as _, Utc};

use super::{
    BuildIdentity, ErrorDetails, LaunchKind, LocalDiagnostics, RedactionContext, RuntimeDetails,
};

fn snapshot() -> LocalDiagnostics {
    LocalDiagnostics::new(
        BuildIdentity::new("subspace", "2.3.4")
            .with_build("beta", "abc123")
            .with_platform("macos", "aarch64"),
        "1.16.0",
    )
    .with_os_version("15.2")
    .with_runtime(RuntimeDetails::new(
        "/Users/test-person/Apps/Subspace",
        LaunchKind::Desktop,
        "login shell /Users/test-person/bin/zsh",
        Some("/Users/test-person/bin/az".into()),
    ))
    .with_redaction(RedactionContext::new(
        Some("/Users/test-person".into()),
        ["1".into()],
    ))
}

#[test]
fn local_copy_orders_error_build_platform_and_runtime_facts() {
    let error = ErrorDetails::new("Azure CLI missing", "Install Azure CLI and retry.", "start")
        .with_code("AZ_MISSING")
        .with_recorded_at(Utc.with_ymd_and_hms(2026, 11, 9, 12, 0, 0).unwrap());
    let copy = snapshot().local_copy(Some(&error));
    assert_eq!(
        copy.markdown(),
        concat!(
            "## Error details\n\n",
            "- Title: Azure CLI missing\n",
            "- Message: Install Azure CLI and retry.\n",
            "- Code: AZ_MISSING\n",
            "- Stage: start\n",
            "- Occurred at (UTC): 2026-11-09T12:00:00Z\n\n",
            "## Diagnostics\n\n",
            "- Product: subspace\n",
            "- App version: 2.3.4\n",
            "- Build channel: beta\n",
            "- Commit: abc123\n",
            "- Subspace Framework version: 1.16.0\n",
            "- OS: macos\n",
            "- OS version: 15.2\n",
            "- Architecture: aarch64\n",
            "- Executable: ~/Apps/Subspace\n",
            "- Launch: desktop\n",
            "- PATH source: login shell ~/bin/zsh\n",
            "- Azure CLI: ~/bin/az"
        )
    );
}
