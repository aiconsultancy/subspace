use std::process::ExitCode;

use subspace_product::{BuildInfo, ProductContext, fixture};

use super::super::{ProductBuild, launch_after_path};

fn fixture_build() -> ProductBuild {
    let mut descriptor = fixture::descriptor();
    descriptor.id = "example-app".into();
    descriptor.binary_name = "example-app".into();
    descriptor.boards.truncate(1);
    descriptor.update = None;
    ProductBuild::new(descriptor, "1.0.0", BuildInfo::UNSTAMPED)
}

fn hook(args: &[String]) -> Option<ExitCode> {
    assert_eq!(args, ["service", "fixture"]);
    Some(ExitCode::from(7))
}

fn command(
    _: &ProductContext,
    _: &[subspace_mcp::Board],
    _: &subspace_agent_tools::ToolEnvironment,
    _: &[String],
) -> ExitCode {
    panic!("startup must exit before constructing a tool environment")
}

// Windows takes the home folder from the profile, not `HOME`, so only Unix
// can point the child at a fixture configuration.
#[cfg(unix)]
#[test]
fn pre_startup_exits_before_configuration_loads() {
    if std::env::var_os("EXAMPLE_STARTUP_CHILD").is_some() {
        let mut build = fixture_build();
        assert!(
            crate::prepare_context(build.descriptor.clone(), build.version)
                .unwrap_err()
                .contains("invalid product configuration JSON")
        );
        build.pre_startup = Some(hook);
        let result = launch_after_path(
            build,
            vec![],
            &[],
            command,
            &["service".into(), "fixture".into()],
        );
        assert_eq!(result, ExitCode::from(7));
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let config = home
        .path()
        .join(".config")
        .join(subspace_product::Channel::current().name("example-app"));
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("config.json"), "invalid JSON").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "startup::tests::pre_startup::pre_startup_exits_before_configuration_loads",
            "--nocapture",
        ])
        .env("EXAMPLE_STARTUP_CHILD", "1")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("running 1 test"), "{stdout}");
}

#[test]
fn pre_startup_runs_before_theme_validation_and_none_continues_startup() {
    let mut build = fixture_build();
    build.default_theme_override = Some("");
    build.pre_startup = Some(hook);
    assert_eq!(
        launch_after_path(
            build,
            vec![],
            &[],
            command,
            &["service".into(), "fixture".into()]
        ),
        ExitCode::from(7)
    );
    let mut build = fixture_build();
    build.default_theme_override = Some("");
    build.pre_startup = Some(|_| None);
    assert_eq!(
        launch_after_path(build, vec![], &[], command, &[]),
        ExitCode::FAILURE
    );
}

#[test]
fn a_new_product_shows_the_navigator_title_by_default() {
    assert!(fixture_build().navigator_title);
}
