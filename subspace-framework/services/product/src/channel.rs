//! The release channel a build belongs to: stable, or one pre-release type.
//! The build sets it once with `SUBSPACE_RELEASE_CHANNEL`; every crate reads it
//! through [`Channel::current`]. Each pre-release type gets its own display
//! name, bundle id and data locations, so it can run beside stable.

/// Stable, or a pre-release type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Stable,
    Nightly,
    Alpha,
    Beta,
}

/// The channel of this build. An unknown value stops compilation, so a
/// mistyped channel can never ship as stable.
const BUILD_CHANNEL: Channel = match Channel::parse(option_env!("SUBSPACE_RELEASE_CHANNEL")) {
    Some(channel) => channel,
    None => panic!(
        "SUBSPACE_RELEASE_CHANNEL must be unset, empty, `stable`, `nightly`, `alpha` or `beta`"
    ),
};

impl Channel {
    /// Every channel, stable first.
    pub const ALL: [Self; 4] = [Self::Stable, Self::Nightly, Self::Alpha, Self::Beta];

    /// The channel this binary was built for.
    pub const fn current() -> Self {
        BUILD_CHANNEL
    }

    /// The channel for a `SUBSPACE_RELEASE_CHANNEL` value, or `None` when the
    /// value is not a channel.
    pub const fn parse(value: Option<&str>) -> Option<Self> {
        let Some(value) = value else {
            return Some(Self::Stable);
        };
        match value.as_bytes() {
            b"" | b"stable" => Some(Self::Stable),
            b"nightly" => Some(Self::Nightly),
            b"alpha" => Some(Self::Alpha),
            b"beta" => Some(Self::Beta),
            _ => None,
        }
    }

    /// `stable`, `nightly`, `alpha` or `beta`.
    // qual:allow(dry, repeated_matches) reason: "Each enum in this group has a single string mapping of its own; rustqual groups the unrelated enums only because their four-arm variant-to-text matches share one shape."
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
            Self::Alpha => "alpha",
            Self::Beta => "beta",
        }
    }

    /// True for a pre-release type.
    pub const fn is_prerelease(self) -> bool {
        !matches!(self, Self::Stable)
    }

    /// A per-channel file or directory name: `stable` unchanged, otherwise
    /// `<stable>-<type>`, such as `subspace-nightly`.
    pub fn name(self, stable: &str) -> String {
        match self {
            Self::Stable => stable.to_owned(),
            other => format!("{stable}-{}", other.as_str()),
        }
    }

    /// `Subspace` for stable, `Subspace (Nightly)` for a type: the type in
    /// title case.
    pub fn display_name(self, stable: &str) -> String {
        match self {
            Self::Stable => stable.to_owned(),
            other => {
                let (first, rest) = other.as_str().split_at(1);
                format!("{stable} ({}{rest})", first.to_ascii_uppercase())
            }
        }
    }

    /// `com.example.subspace` for stable, `com.example.subspace.nightly`
    /// for a type.
    pub fn bundle_id(self, stable: &str) -> String {
        match self {
            Self::Stable => stable.to_owned(),
            other => format!("{stable}.{}", other.as_str()),
        }
    }
}

/// Release asset stem for a macOS app archive: the words of the display
/// name joined with `-`, such as `Subspace` or `Subspace-Nightly` for
/// `Subspace (Nightly)`. It has no spaces or parentheses.
pub fn asset_stem(display_name: &str) -> String {
    display_name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::{Channel, asset_stem};

    #[test]
    fn build_values_parse_to_channels_and_anything_else_is_rejected() {
        assert_eq!(Channel::parse(None), Some(Channel::Stable));
        assert_eq!(Channel::parse(Some("")), Some(Channel::Stable));
        assert_eq!(Channel::parse(Some("stable")), Some(Channel::Stable));
        assert_eq!(Channel::parse(Some("nightly")), Some(Channel::Nightly));
        assert_eq!(Channel::parse(Some("alpha")), Some(Channel::Alpha));
        assert_eq!(Channel::parse(Some("beta")), Some(Channel::Beta));
        for invalid in ["Nightly", "rc", " beta", "stable ", "dev"] {
            assert_eq!(Channel::parse(Some(invalid)), None, "{invalid:?}");
        }
    }

    #[test]
    fn every_channel_round_trips_through_its_name() {
        for channel in Channel::ALL {
            assert_eq!(Channel::parse(Some(channel.as_str())), Some(channel));
            assert_eq!(channel.is_prerelease(), channel != Channel::Stable);
        }
    }

    #[test]
    fn stable_names_are_unchanged() {
        let stable = Channel::Stable;
        assert_eq!(stable.name("subspace"), "subspace");
        assert_eq!(stable.display_name("Subspace"), "Subspace");
        assert_eq!(
            stable.bundle_id("com.example.subspace"),
            "com.example.subspace"
        );
        assert_eq!(asset_stem(&stable.display_name("Subspace")), "Subspace");
    }

    #[test]
    fn each_type_names_follow_the_channel_contract() {
        let cases = [
            (Channel::Nightly, "Nightly"),
            (Channel::Alpha, "Alpha"),
            (Channel::Beta, "Beta"),
        ];
        for (channel, title) in cases {
            let kind = channel.as_str();
            assert_eq!(channel.name("subspace"), format!("subspace-{kind}"));
            assert_eq!(
                channel.display_name("Subspace"),
                format!("Subspace ({title})")
            );
            assert_eq!(
                channel.bundle_id("com.example.subspace"),
                format!("com.example.subspace.{kind}")
            );
            assert_eq!(
                asset_stem(&channel.display_name("Subspace")),
                format!("Subspace-{title}")
            );
        }
    }

    #[test]
    fn the_current_channel_is_the_build_value() {
        let expected = Channel::parse(option_env!("SUBSPACE_RELEASE_CHANNEL"));
        assert_eq!(Some(Channel::current()), expected);
    }
}
