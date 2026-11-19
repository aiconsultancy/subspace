//! Release versions of one channel (REQ-02, REQ-30 to REQ-32). Stable
//! accepts only strict `X.Y.Z`; a pre-release type accepts only
//! `X.Y.Z-<type>.…`. Anything else never compares as a release, so a stable
//! build cannot select a pre-release and a nightly cannot select stable or
//! another type. Versions of one channel compare by semver precedence.

use anyhow::{Result, ensure};
use semver::Version;
use subspace_product::Channel;

/// The parsed version when `value` is a release of `channel`: no leading
/// `v`, no leading zero, no build metadata, and a pre-release part that is
/// empty on stable or starts with the type otherwise.
pub(crate) fn release_version(channel: Channel, value: &str) -> Option<Version> {
    let version = Version::parse(value).ok()?;
    let kind = version.pre.as_str().split('.').next().unwrap_or_default();
    let expected = if channel.is_prerelease() {
        channel.as_str()
    } else {
        ""
    };
    (kind == expected && version.build.is_empty()).then_some(version)
}

/// True when `candidate` is a release of `channel` above `current`. A
/// `current` that is not a release of the channel counts as older, so a
/// development build still sees published releases as newer. Integration
/// builds opt out regardless of the selected channel.
pub(crate) fn is_newer(channel: Channel, candidate: &str, current: &str) -> bool {
    if subspace_product::is_integration_version(current) {
        return false;
    }
    match release_version(channel, candidate) {
        Some(candidate) => {
            release_version(channel, current).is_none_or(|current| candidate > current)
        }
        None => false,
    }
}

/// The version text of a user-supplied `--version`, with an optional leading
/// `v` removed. It must be a release of `channel`.
pub(crate) fn require_release_version(channel: Channel, value: &str) -> Result<String> {
    let value = value.strip_prefix('v').unwrap_or(value);
    ensure!(
        release_version(channel, value).is_some(),
        "{}",
        not_in_channel(channel, value)
    );
    Ok(value.to_owned())
}

fn not_in_channel(channel: Channel, value: &str) -> String {
    if channel.is_prerelease() {
        let kind = channel.as_str();
        format!(
            "version {value:?} is not a {kind} release; this build installs only {kind} versions, such as 1.2.3-{kind}.202601010200.g0123abc"
        )
    } else {
        format!("version {value:?} is not of the form 1.2.3")
    }
}

#[cfg(test)]
mod tests {
    use subspace_product::Channel;

    use super::{is_newer, release_version, require_release_version};

    const NIGHTLY: &str = "1.10.2-nightly.202611010200.ga43a333";
    const NEWER_NIGHTLY: &str = "1.10.2-nightly.202611020200.gb0c1d2e";
    const INTEGRATION: &str = "0.3.0-feat-subspace-desktop.202611151345.gabc1234";
    const INTEGRATIONS: [&str; 3] = [
        INTEGRATION,
        "0.3.0-nightly-fix.202611151345.gabc1234",
        "0.3.0-beta-2.202611151345.gabc1234",
    ];

    #[test]
    fn stable_rejects_integration_releases() {
        let integration = semver::Version::parse(INTEGRATION).unwrap();
        assert!(integration > semver::Version::parse("0.2.1").unwrap());
        assert!(integration < semver::Version::parse("0.3.0").unwrap());
        for version in INTEGRATIONS {
            assert!(release_version(Channel::Stable, version).is_none());
            assert!(!is_newer(Channel::Stable, version, "0.2.1"));
        }
    }

    #[test]
    fn nightly_rejects_integration_releases() {
        for version in INTEGRATIONS {
            assert!(release_version(Channel::Nightly, version).is_none());
            assert!(!is_newer(Channel::Nightly, version, NIGHTLY));
        }
    }

    #[test]
    fn alpha_rejects_integration_releases() {
        for version in INTEGRATIONS {
            assert!(release_version(Channel::Alpha, version).is_none());
            assert!(!is_newer(
                Channel::Alpha,
                version,
                "0.3.0-alpha.202611151345.gabc1234"
            ));
        }
    }

    #[test]
    fn beta_rejects_integration_releases() {
        for version in INTEGRATIONS {
            assert!(release_version(Channel::Beta, version).is_none());
            assert!(!is_newer(
                Channel::Beta,
                version,
                "0.3.0-beta.202611151345.gabc1234"
            ));
        }
    }

    #[test]
    fn integration_builds_never_offer_channel_updates_or_a_stable_downgrade() {
        for current in INTEGRATIONS {
            for (channel, candidate) in [
                (Channel::Stable, "0.2.1"),
                (Channel::Stable, "9.0.0"),
                (Channel::Nightly, NEWER_NIGHTLY),
                (Channel::Alpha, "9.0.0-alpha.202611151345.gabc1234"),
                (Channel::Beta, "9.0.0-beta.202611151345.gabc1234"),
            ] {
                assert!(!is_newer(channel, candidate, current));
            }
        }
    }

    fn stable(value: &str) -> Option<[u64; 3]> {
        release_version(Channel::Stable, value).map(|v| [v.major, v.minor, v.patch])
    }

    #[test]
    fn only_three_decimal_parts_without_a_leading_zero_parse() {
        assert_eq!(stable("1.2.3"), Some([1, 2, 3]));
        assert_eq!(stable("0.0.0"), Some([0, 0, 0]));
        assert_eq!(stable("10.20.30"), Some([10, 20, 30]));
        for invalid in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.02.3",
            "v1.2.3",
            "+1.2.3",
            "1.2.3-beta",
            "1.2.-3",
            "1..3",
            "1.2.",
            "1.2.3+build",
        ] {
            assert_eq!(stable(invalid), None, "{invalid:?} must not parse");
        }
    }

    #[test]
    fn newer_compares_numerically_and_refuses_invalid_candidates() {
        let newer = |candidate, current| is_newer(Channel::Stable, candidate, current);
        assert!(newer("1.10.0", "1.9.9"));
        assert!(newer("1.0.0", "0.99.99"));
        assert!(newer("0.1.0", "0.0.0-dev"));
        assert!(!newer("1.0.0", "1.0.0"));
        assert!(!newer("0.9.0", "1.0.0"));
        assert!(!newer("1.2.3-beta", "0.0.0"));
        assert!(!newer("bad", "invalid"));
    }

    #[test]
    fn a_type_accepts_only_its_own_pre_releases() {
        assert!(release_version(Channel::Nightly, NIGHTLY).is_some());
        assert!(release_version(Channel::Beta, "1.13.2-beta.202611010200.g0123456").is_some());
        for other in [
            "1.10.2",
            "1.10.2-beta.202611010200.ga43a333",
            "1.10.2-nightlyx.1",
            "1.10.2-nightly.202611010200.ga43a333+meta",
            "v1.10.2-nightly.1",
        ] {
            assert!(
                release_version(Channel::Nightly, other).is_none(),
                "{other}"
            );
        }
        assert!(release_version(Channel::Stable, NIGHTLY).is_none());
    }

    #[test]
    fn a_type_compares_by_semver_precedence_within_the_type() {
        let newer = |candidate, current| is_newer(Channel::Nightly, candidate, current);
        assert!(newer(NEWER_NIGHTLY, NIGHTLY));
        assert!(newer("1.10.3-nightly.202611010100.g0000001", NEWER_NIGHTLY));
        assert!(!newer(NIGHTLY, NEWER_NIGHTLY));
        assert!(!newer(NIGHTLY, NIGHTLY));
        // A stable or other-type candidate is never an update (AE-23).
        assert!(!newer("1.10.2", NIGHTLY));
        assert!(!newer("2.0.0", NIGHTLY));
        assert!(!newer("1.10.3-beta.202611030200.g0000002", NIGHTLY));
        // An unstamped type build still sees its type's releases as newer.
        assert!(newer(NIGHTLY, "1.10.1"));
    }

    #[test]
    fn a_requested_version_drops_a_leading_v_and_rejects_other_shapes() {
        let stable = |value| require_release_version(Channel::Stable, value);
        assert_eq!(stable("v1.2.0").unwrap(), "1.2.0");
        assert_eq!(stable("1.2.0").unwrap(), "1.2.0");
        let error = stable("1.2").unwrap_err();
        assert!(error.to_string().contains("1.2.3"), "{error}");
        assert!(stable(NIGHTLY).is_err());
    }

    #[test]
    fn a_type_build_refuses_a_pinned_stable_or_other_type_version() {
        assert_eq!(
            require_release_version(Channel::Nightly, &format!("v{NIGHTLY}")).unwrap(),
            NIGHTLY
        );
        for pinned in ["1.10.1", "1.10.2-beta.202611010200.ga43a333"] {
            let error = require_release_version(Channel::Nightly, pinned).unwrap_err();
            assert!(
                error.to_string().contains("only nightly versions"),
                "{error}"
            );
        }
    }
}
