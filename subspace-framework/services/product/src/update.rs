//! The release stream an app build updates from, and the reserved `upgrade`
//! root command that the Subspace Framework mounts when an app has one.

use crate::channel::Channel;
use crate::error::ProductError;

/// The reserved root command name. No board or alias may use it.
pub const UPGRADE_COMMAND: &str = "upgrade";

/// usage.jdx.dev KDL for the `upgrade` command.
pub const UPGRADE_USAGE: &str = r#"cmd "upgrade" effect="write" help="Check for or install a release of this app" {
  flag "--check" help="Report the newest release without installing it"
  flag "--version <VERSION>" help="Install the release VERSION, such as 1.2.0, even when it is older"
}"#;

/// GitHub release stream for one app build. An app without one has no update
/// features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSource {
    /// `owner/name` of the GitHub repository that publishes the releases.
    pub repository: String,
    /// Tag text before `X.Y.Z`, such as `subspace-desktop-v`.
    pub tag_prefix: String,
    /// Plain archive prefix, such as `subspace` in
    /// `subspace-1.2.0-x86_64-unknown-linux-gnu.tar.gz`.
    pub archive_prefix: String,
    /// macOS app archive stem, such as `Subspace` in
    /// `Subspace_1.2.0_darwin_arm64.zip` or `Subspace-Nightly` for a nightly.
    /// It has no spaces; the bundle directory inside is `<display name>.app`.
    pub app_bundle: String,
    /// App-specific GitHub token variable, read before `GITHUB_TOKEN`.
    pub token_variable: String,
    /// The channel this build updates within. Stable selects only stable
    /// releases; a type selects only published pre-releases of that type.
    pub channel: Channel,
    /// Whether this build installs only its own archive, as a variant built
    /// with its own asset names does: a newer release that does not publish
    /// that archive is no update for it, rather than one whose install
    /// fails. False keeps the ordinary behaviour.
    pub exclusive: bool,
}

/// Returns the source unchanged when every field is safe to put in a URL,
/// file name or environment lookup.
pub fn validate_update_source(source: &UpdateSource) -> Result<UpdateSource, ProductError> {
    let checks = [
        (&source.repository, repository_is_safe(&source.repository)),
        (&source.tag_prefix, tag_prefix_is_safe(&source.tag_prefix)),
        (&source.archive_prefix, name_is_safe(&source.archive_prefix)),
        (&source.app_bundle, name_is_safe(&source.app_bundle)),
        (
            &source.token_variable,
            variable_is_safe(&source.token_variable),
        ),
    ];
    match checks.into_iter().find(|(_, safe)| !safe) {
        Some((name, _)) => Err(ProductError::UnsafeUpdateSource { name: name.clone() }),
        None => Ok(source.clone()),
    }
}

fn name_is_safe(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !value.contains("..")
}

pub(crate) fn validate_release_repository(value: &str) -> Result<String, ProductError> {
    if repository_is_safe(value) {
        Ok(value.to_owned())
    } else {
        Err(ProductError::UnsafeReleaseRepository { name: value.into() })
    }
}

pub(crate) fn require_release_repository(
    source: &UpdateSource,
    release_repository: &str,
) -> Result<(), ProductError> {
    if source.repository == release_repository {
        Ok(())
    } else {
        Err(ProductError::UpdateRepositoryMismatch {
            repository: source.repository.clone(),
            release_repository: release_repository.into(),
        })
    }
}

fn repository_is_safe(value: &str) -> bool {
    matches!(value.split('/').collect::<Vec<_>>()[..], [owner, name] if name_is_safe(owner) && name_is_safe(name))
}

fn tag_prefix_is_safe(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && value
            .split('/')
            .all(|part| part.is_empty() || name_is_safe(part))
        && !value.contains("//")
}

fn variable_is_safe(value: &str) -> bool {
    value.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::{UpdateSource, validate_update_source};
    use crate::error::ProductError;

    fn source() -> UpdateSource {
        UpdateSource {
            repository: "example-org/example-app".into(),
            tag_prefix: "subspace-desktop-v".into(),
            archive_prefix: "subspace".into(),
            app_bundle: "Subspace".into(),
            token_variable: "SUBSPACE_GITHUB_TOKEN".into(),
            channel: crate::Channel::Stable,
            exclusive: false,
        }
    }

    #[test]
    fn a_complete_source_is_accepted_unchanged() {
        assert_eq!(validate_update_source(&source()).unwrap(), source());
    }

    #[test]
    fn each_unsafe_field_is_rejected_by_name() {
        let cases: [(fn(&mut UpdateSource) -> &mut String, &str); 9] = [
            (|s| &mut s.repository, "no-slash"),
            (|s| &mut s.repository, "a/b/c"),
            (|s| &mut s.repository, "owner/../x"),
            (|s| &mut s.tag_prefix, ""),
            (|s| &mut s.tag_prefix, "/abs/v"),
            (|s| &mut s.tag_prefix, "tools/../v"),
            (|s| &mut s.archive_prefix, "a b"),
            (|s| &mut s.app_bundle, "Sub/space"),
            (|s| &mut s.token_variable, "lower_case"),
        ];
        for (field, value) in cases {
            let mut candidate = source();
            *field(&mut candidate) = value.into();
            assert_eq!(
                validate_update_source(&candidate),
                Err(ProductError::UnsafeUpdateSource { name: value.into() })
            );
        }
    }
}
