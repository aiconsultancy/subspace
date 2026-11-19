//! Download-only integration builds retain the stable product identity.

use semver::Version;

const STAMP_LENGTH: usize = 12;
const COMMIT_LENGTH: usize = 8;

/// Whether a version has the branch pre-release shape emitted by the local
/// release planner. Ordinary development versions retain their update policy.
pub fn is_integration_version(value: &str) -> bool {
    Version::parse(value).is_ok_and(integration_prerelease)
}

fn integration_prerelease(version: Version) -> bool {
    let parts: Vec<_> = version.pre.as_str().split('.').collect();
    matches!(parts.as_slice(), [slug, id, commit]
        if version.build.is_empty()
            && branch_slug(slug)
            && timestamp(id)
            && commit_id(commit))
}

fn branch_slug(slug: &str) -> bool {
    !matches!(slug, "stable" | "nightly" | "alpha" | "beta")
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn timestamp(value: &str) -> bool {
    value.len() == STAMP_LENGTH
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn commit_id(commit: &str) -> bool {
    commit.len() == COMMIT_LENGTH
        && commit.starts_with('g')
        && commit[1..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::is_integration_version;

    #[test]
    fn branch_versions_are_integration_builds_and_channels_are_not() {
        for slug in [
            "feat-subspace-desktop",
            "nightly-fix",
            "beta-2",
            "fix-123",
            "123",
            "--topic",
        ] {
            assert!(is_integration_version(&format!(
                "0.3.0-{slug}.202611151345.gabc1234"
            )));
        }
        for slug in ["stable", "nightly", "alpha", "beta", "Feat", "01", ""] {
            assert!(!is_integration_version(&format!(
                "0.3.0-{slug}.202611151345.gabc1234"
            )));
        }
    }

    #[test]
    fn other_versions_do_not_disable_updates() {
        for version in [
            "0.2.1",
            "0.3.0-dev",
            "0.3.0-feat-topic.1.gabc1234",
            "0.3.0-feat-topic.202611151345.gABC1234",
            "0.3.0-feat-topic.202611151345.gabc1234+build",
        ] {
            assert!(!is_integration_version(version), "{version}");
        }
    }
}
