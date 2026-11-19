//! A child test process isolates synthetic credentials from parallel tests.
use super::pi_process;

#[tokio::test]
async fn spawned_pi_filters_service_credentials_but_keeps_provider_authentication() {
    const MARKER: &str = "SUBSPACE_TEST_PI_ENV_CHILD";
    const SERVICES: [&str; 3] = ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"];
    if std::env::var_os(MARKER).is_some() {
        let mut command = pi_process(std::path::Path::new("/bin/sh"));
        let checks = SERVICES
            .iter()
            .map(|name| format!("test -z \"${{{name}+present}}\" || exit 1\n"))
            .collect::<String>();
        command.args(["-c", &format!("{checks}test \"$OPENAI_API_KEY\" = test-only-provider-key || exit 2\nprintf filtered")]);
        let output = command.output().await.unwrap();
        assert!(
            output.status.success(),
            "service credentials leaked or provider auth was removed"
        );
        assert_eq!(output.stdout, b"filtered");
        return;
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env(MARKER, "1")
        .env("OPENAI_API_KEY", "test-only-provider-key");
    for name in SERVICES {
        child.env(name, "test-only-service-value");
    }
    let output = child.args(["--exact", "runtime::pi::process_tests::environment_tests::spawned_pi_filters_service_credentials_but_keeps_provider_authentication", "--nocapture"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "the isolated test must actually run"
    );
}
