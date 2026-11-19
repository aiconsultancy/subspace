//! The product identity and build the status bar, About and `--version`
//! show. The product's build script stamps [`BuildInfo`].

use subspace_product::{BuildInfo, BuildKind, ProductContext, ProductDescriptor, SUBSPACE_VERSION};

pub fn status_text(context: &ProductContext) -> String {
    version_text_for(
        &context.product.display_name,
        &shown_version(&context.version, &context.build),
    )
}

pub fn version_text(descriptor: &ProductDescriptor, version: &str, build: &BuildInfo) -> String {
    version_text_for(&descriptor.display_name, &shown_version(version, build))
}

/// The version as shown to people. Only display text carries the marker;
/// update checks compare the real version.
fn shown_version(version: &str, build: &BuildInfo) -> String {
    marked_version(version, build.kind, build.host, build.unix_time)
}

/// `version` unchanged for a release build, else
/// `<version>-<kind>-<host>-<unix time>`, such as `0.2.1-dev-studio-1763290000`,
/// so a local build is never mistaken for a release.
fn marked_version(version: &str, kind: BuildKind, host: &str, unix_time: u64) -> String {
    if kind == BuildKind::Release {
        version.to_owned()
    } else {
        format!("{version}-{}-{host}-{unix_time}", kind.as_str())
    }
}

fn version_text_for(name: &str, version: &str) -> String {
    format!("{name} v{version}")
}

pub fn about_text(descriptor: &ProductDescriptor, version: &str, build: &BuildInfo) -> String {
    format_about(
        &version_text(descriptor, version, build),
        SUBSPACE_VERSION,
        build,
    )
}

pub fn about_text_for(context: &ProductContext) -> String {
    format_about(
        &status_text(context),
        &context.subspace_version,
        &context.build,
    )
}

/// The app's version line, then the Framework's under its own fixed name,
/// whatever the app is called.
fn format_about(version: &str, framework_version: &str, build: &BuildInfo) -> String {
    let repository = build
        .repository
        .map(|url| format!("Repository: {url}\n"))
        .unwrap_or_default();
    format!(
        "{version}\nSubspace Framework v{framework_version}\n\n{repository}Git commit: {}\nGit branch: {}\nBuilt: {}\nRust target: {}\nProfile: {}",
        build.commit_label(),
        build.branch,
        build.timestamp,
        build.target,
        profile_name(build.profile)
    )
}

fn profile_name(profile: &str) -> &str {
    if profile == "release" {
        "Release"
    } else {
        "Debug"
    }
}

#[cfg(test)]
mod tests {
    use super::{
        about_text, about_text_for, format_about, marked_version, profile_name, status_text,
        version_text, version_text_for,
    };
    use subspace_product::{
        BuildInfo, BuildKind, Channel, PlatformEnv, ProductContext, SUBSPACE_VERSION, fixture,
        validate_product,
    };

    const STAMPED: BuildInfo = BuildInfo {
        repository: Some("https://example.com/subspace"),
        commit: "abc1234",
        dirty: true,
        branch: "main",
        timestamp: "2026-11-17T09:30:00Z",
        unix_time: 1_763_290_000,
        target: "aarch64-apple-darwin",
        profile: "debug",
        kind: BuildKind::Test,
        host: "studio",
    };

    #[test]
    fn a_type_build_shows_its_display_name_and_full_version() {
        let version = "1.13.2-beta.202611010200.g0123456";
        let nightly = fixture::descriptor_for(Channel::Nightly);
        assert_eq!(version_text_for("Subspace", "0.1.0"), "Subspace v0.1.0");
        assert_eq!(
            version_text_for(&nightly.display_name, version),
            format!("Subspace (Nightly) v{version}")
        );
    }

    #[test]
    fn only_release_builds_show_the_bare_version() {
        assert_eq!(
            marked_version("0.2.1", BuildKind::Release, "studio", 1_763_290_000),
            "0.2.1"
        );
        assert_eq!(
            marked_version("0.2.1", BuildKind::Dev, "studio", 1_763_290_000),
            "0.2.1-dev-studio-1763290000"
        );
        assert_eq!(
            marked_version("0.2.1", BuildKind::Test, "capture", 1_763_290_000),
            "0.2.1-test-capture-1763290000"
        );
    }

    #[test]
    fn a_test_build_marks_its_version_as_a_test_build() {
        let stable = fixture::descriptor_for(Channel::Stable);
        assert_eq!(
            version_text(&stable, "0.2.1", &STAMPED),
            "Subspace v0.2.1-test-studio-1763290000"
        );
        let mut context = subspace_product::ProductContext::new(
            subspace_product::validate_product(&stable).unwrap(),
            subspace_product::PlatformEnv::from_process().unwrap(),
        );
        context.version = "0.2.1".into();
        context.build = STAMPED;
        assert_eq!(
            status_text(&context),
            "Subspace v0.2.1-test-studio-1763290000"
        );
    }

    fn context(display_name: &str, subspace_version: &str, build: BuildInfo) -> ProductContext {
        let mut descriptor = fixture::descriptor_for(Channel::Stable);
        descriptor.display_name = display_name.into();
        descriptor.update = None;
        let mut context = ProductContext::new(
            validate_product(&descriptor).unwrap(),
            PlatformEnv::from_process().unwrap(),
        );
        context.version = "1.2.0".into();
        context.subspace_version = subspace_version.into();
        context.build = build;
        context
    }

    #[test]
    fn the_about_dialog_shows_the_app_then_the_framework_version() {
        let text = about_text_for(&context("Harbor", "0.1.0", STAMPED));
        assert!(text.starts_with(
            "Harbor v1.2.0-test-studio-1763290000\nSubspace Framework v0.1.0\n\n\
             Repository: https://example.com/subspace\nGit commit: abc1234-dirty\n"
        ));
        let build = BuildInfo {
            repository: None,
            kind: BuildKind::Release,
            ..STAMPED
        };
        let text = about_text_for(&context("Harbor", "0.3.0", build));
        assert!(text.starts_with("Harbor v1.2.0\nSubspace Framework v0.3.0\n\nGit commit: "));
    }

    #[test]
    fn about_on_the_command_line_shows_this_framework_version() {
        let stable = fixture::descriptor_for(Channel::Stable);
        let text = about_text(&stable, "0.8.0", &STAMPED);
        assert!(text.starts_with(&format!(
            "Subspace v0.8.0-test-studio-1763290000\nSubspace Framework v{SUBSPACE_VERSION}\n\n\
             Repository: https://example.com/subspace\n"
        )));
        let build = BuildInfo {
            repository: None,
            ..STAMPED
        };
        let text = about_text(&stable, "0.8.0", &build);
        assert!(text.starts_with(&format!(
            "Subspace v0.8.0-test-studio-1763290000\nSubspace Framework v{SUBSPACE_VERSION}\n\nGit commit: "
        )));
    }

    #[test]
    fn about_names_the_repository_and_build_fields() {
        let text = format_about("Subspace v0.1.0", "0.1.0", &STAMPED);
        assert!(text.contains("Repository: https://example.com/subspace\n"));
        assert!(!text.contains("Contact"));
        assert!(text.contains("Git commit: abc1234-dirty\n"));
        assert!(text.contains("Git branch: main\n"));
        assert!(text.contains("Built: 2026-11-17T09:30:00Z\n"));
        assert!(text.contains("Rust target: aarch64-apple-darwin"));
        assert_eq!(profile_name("release"), "Release");
        assert_eq!(profile_name("debug"), "Debug");
    }

    #[test]
    fn about_without_a_repository_has_no_repository_line() {
        let build = BuildInfo {
            repository: None,
            ..STAMPED
        };
        let text = format_about("Subspace v0.1.0", "0.1.0", &build);
        assert!(!text.contains("Repository"));
        assert!(text.starts_with("Subspace v0.1.0\nSubspace Framework v0.1.0\n\nGit commit: "));
    }
}
