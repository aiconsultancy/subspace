//! Subspace's build configuration: the GitHub OAuth App that Sign in with
//! GitHub uses and the repository Report issue files into. The Framework has
//! neither; the app passes both through `ProductBuild`. The defaults live
//! here, and the build environment overrides each one. `build.rs` prints the
//! directives; the app's tests include this file to test it.

/// Subspace's GitHub OAuth App. Public: it appears in every authorize URL.
pub(crate) const GITHUB_OAUTH_CLIENT_ID: &str = "Ov23lihtCHEoDujzEB6y";
/// The repository Report issue files into.
pub(crate) const REPORT_ISSUE_REPOSITORY: &str = subspace_descriptor::RELEASE_REPOSITORY;

/// Overrides the client id; empty builds without Sign in with GitHub.
pub(crate) const GITHUB_OAUTH_CLIENT_ID_ENV: &str = "SUBSPACE_GITHUB_OAUTH_CLIENT_ID";
/// The OAuth App's client secret, never in the repository. The release
/// scripts read it from the login keychain. `main.rs` reads it with
/// `option_env!`, so it never passes through the build script's output,
/// which Cargo keeps and prints when a build fails. Rustc still records its
/// value in `target/` dep-info (`deps/*.d`), and Cargo may retain it in
/// fingerprints. Treat the build target directory like the compiled binary.
pub(crate) const GITHUB_OAUTH_CLIENT_SECRET_ENV: &str = "SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET";
/// Overrides the repository; empty builds without Report issue.
pub(crate) const REPORT_ISSUE_REPOSITORY_ENV: &str = "SUBSPACE_REPORT_ISSUE_REPOSITORY";

/// The build environment's overrides; `None` is unset, which keeps the
/// default. An empty value is an override that turns the feature off.
#[derive(Default)]
pub(crate) struct Overrides {
    pub(crate) github_oauth_client_id: Option<String>,
    pub(crate) report_issue_repository: Option<String>,
}

/// Every `cargo:` directive: re-run when any of the three variables
/// changes, and the resolved client id and repository as
/// `SUBSPACE_BUILD_GITHUB_OAUTH_CLIENT_ID` and
/// `SUBSPACE_BUILD_REPORT_ISSUE_REPOSITORY`.
pub(crate) fn directives(overrides: &Overrides) -> Vec<String> {
    let watched = [
        GITHUB_OAUTH_CLIENT_ID_ENV,
        GITHUB_OAUTH_CLIENT_SECRET_ENV,
        REPORT_ISSUE_REPOSITORY_ENV,
    ]
    .map(|name| format!("cargo:rerun-if-env-changed={name}"));
    let values = [
        (
            "GITHUB_OAUTH_CLIENT_ID",
            resolve(&overrides.github_oauth_client_id, GITHUB_OAUTH_CLIENT_ID),
        ),
        (
            "REPORT_ISSUE_REPOSITORY",
            resolve(&overrides.report_issue_repository, REPORT_ISSUE_REPOSITORY),
        ),
    ]
    .map(|(name, value)| format!("cargo:rustc-env=SUBSPACE_BUILD_{name}={value}"));
    watched.into_iter().chain(values).collect()
}

/// The override, trimmed and on one line, else the default.
fn resolve(value: &Option<String>, default: &str) -> String {
    value
        .as_deref()
        .map_or(default, str::trim)
        .replace(['\r', '\n'], "")
}

#[cfg(test)]
mod tests {
    use super::{GITHUB_OAUTH_CLIENT_ID, Overrides, REPORT_ISSUE_REPOSITORY, directives};

    fn values(overrides: &Overrides) -> Vec<String> {
        directives(overrides)
            .into_iter()
            .filter_map(|line| line.strip_prefix("cargo:rustc-env=").map(str::to_owned))
            .collect()
    }

    #[test]
    fn subspace_builds_with_its_oauth_app_and_repository_by_default() {
        assert_eq!(
            values(&Overrides::default()),
            [
                format!("SUBSPACE_BUILD_GITHUB_OAUTH_CLIENT_ID={GITHUB_OAUTH_CLIENT_ID}"),
                format!("SUBSPACE_BUILD_REPORT_ISSUE_REPOSITORY={REPORT_ISSUE_REPOSITORY}"),
            ]
        );
    }

    #[test]
    fn the_environment_overrides_each_value_and_empty_turns_it_off() {
        let overrides = Overrides {
            github_oauth_client_id: Some(" Iv1.fixture \n".into()),
            report_issue_repository: Some(String::new()),
        };
        assert_eq!(
            values(&overrides),
            [
                "SUBSPACE_BUILD_GITHUB_OAUTH_CLIENT_ID=Iv1.fixture",
                "SUBSPACE_BUILD_REPORT_ISSUE_REPOSITORY=",
            ]
        );
        let injected = Overrides {
            report_issue_repository: Some("a/b\ncargo:rustc-env=X=1".into()),
            ..Overrides::default()
        };
        assert!(
            directives(&injected)
                .iter()
                .all(|line| !line.contains('\n'))
        );
    }

    #[test]
    fn every_variable_reruns_the_build_and_the_secret_is_never_a_value() {
        let lines = directives(&Overrides::default());
        for name in [
            "SUBSPACE_GITHUB_OAUTH_CLIENT_ID",
            "SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET",
            "SUBSPACE_REPORT_ISSUE_REPOSITORY",
        ] {
            assert!(
                lines.contains(&format!("cargo:rerun-if-env-changed={name}")),
                "{name}"
            );
        }
        assert!(
            lines
                .iter()
                .filter(|line| line.starts_with("cargo:rustc-env="))
                .all(|line| !line.contains("SECRET"))
        );
    }
}
