use subspace_core::diagnostics::{
    BuildIdentity, LaunchKind, LocalDiagnostics, RedactionContext, RuntimeDetails,
};

use super::super::{Diagnostics, Draft, ErrorKind, prepare};
use super::repository;

const ORIGINAL_BODY: &str = "## Description\n\nordinary description\n\n## Type\n\nUnknown\n\n## Diagnostics\n\n- Product: subspace\n- App version: 1.2.3\n- Build channel: nightly\n- Commit: abc123\n- OS: macos\n- Architecture: aarch64\n- Active board: github\n- Theme: one-dark\n- Icon size: small";

fn identity() -> BuildIdentity {
    BuildIdentity::new("subspace", "1.2.3")
        .with_build("nightly", "abc123")
        .with_platform("macos", "aarch64")
}

fn diagnostics() -> Diagnostics {
    Diagnostics::from(identity())
        .with_active_board("github")
        .with_appearance("one-dark", "small")
}

fn local() -> LocalDiagnostics {
    LocalDiagnostics::new(identity(), "LOCAL_FRAMEWORK_NOT_ALLOWED")
        .with_os_version("LOCAL_OS_VERSION_NOT_ALLOWED")
        .with_runtime(RuntimeDetails::new(
            "/Users/fixture/Applications/Test App.app/Contents/MacOS/app",
            LaunchKind::Desktop,
            "login shell /Users/fixture/bin/zsh",
            Some("/Users/fixture/bin/az".into()),
        ))
        .with_redaction(RedactionContext::new(
            Some("/Users/fixture".into()),
            ["fixture".into(), "1".into()],
        ))
}

#[test]
fn paths_default_off_and_removing_an_opt_in_restore_the_exact_original_body() {
    let draft = Draft::new("ordinary description");
    let original = diagnostics();
    assert!(original.paths().is_none());
    assert_eq!(
        prepare(&draft, &original, &repository()).unwrap().body(),
        ORIGINAL_BODY
    );
    let off_again = original
        .with_paths(local().issue_paths(Some("/Users/fixture/config/tunnel.json")))
        .without_paths();
    assert!(off_again.paths().is_none());
    assert_eq!(
        prepare(&draft, &off_again, &repository()).unwrap().body(),
        ORIGINAL_BODY
    );
}

#[test]
fn opt_in_appends_only_the_ordered_path_allowlist_without_identity_validation() {
    let paths = local().issue_paths(Some("/Users/fixture/My Config/tunnel.json"));
    let with_paths = diagnostics().with_paths(paths);
    assert!(with_paths.paths().is_some());
    let report = prepare(
        &Draft::new("ordinary description"),
        &with_paths,
        &repository(),
    )
    .unwrap();
    assert_eq!(
        report.body(),
        concat!(
            "## Description\n\nordinary description\n\n## Type\n\nUnknown\n\n## Diagnostics\n\n",
            "- Product: subspace\n- App version: 1.2.3\n- Build channel: nightly\n- Commit: abc123\n",
            "- OS: macos\n- Architecture: aarch64\n- Active board: github\n- Theme: one-dark\n- Icon size: small\n",
            "- Executable: ~/Applications/Test App.app/Contents/MacOS/app\n",
            "- Launch: desktop\n- PATH source: login shell ~/bin/zsh\n- Azure CLI: ~/bin/az\n",
            "- Configuration file: ~/My Config/tunnel.json"
        )
    );
    assert!(!report.body().contains("LOCAL_FRAMEWORK_NOT_ALLOWED"));
    assert!(!report.body().contains("LOCAL_OS_VERSION_NOT_ALLOWED"));
}

#[test]
fn absent_config_path_is_omitted_and_missing_azure_cli_keeps_its_exact_wording() {
    let snapshot = LocalDiagnostics::new(identity(), "1").with_runtime(RuntimeDetails::new(
        "/opt/test-app",
        LaunchKind::Terminal,
        "inherited",
        None,
    ));
    let report = prepare(
        &Draft::new("ordinary description"),
        &diagnostics().with_paths(snapshot.issue_paths(None)),
        &repository(),
    )
    .unwrap();
    assert!(report.body().ends_with("- Executable: /opt/test-app\n- Launch: terminal\n- PATH source: inherited\n- Azure CLI: not found on the app's PATH"));
    assert!(!report.body().contains("Configuration file"));
}

#[test]
fn opting_into_paths_does_not_relax_existing_identity_validation() {
    let invalid = Diagnostics::new("invalid/product", "1").with_paths(local().issue_paths(None));
    assert_eq!(
        prepare(&Draft::new("description"), &invalid, &repository())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidDiagnostics
    );
}
