//! How a feed is connected to its fleet, in words.

/// How a feed is connected to its fleet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Link {
    /// Opening the stream.
    #[default]
    Connecting,
    /// Events arrive as they happen.
    Live,
    /// Stargate refused the stream; the fleet is read on a timer.
    Polling,
    /// The stream dropped; trying again with backoff.
    Reconnecting,
    /// Stargate refused the token. Nothing more is sent with it.
    TokenRefused,
    /// Stargate refused this network.
    Forbidden,
    /// The address could not be reached; trying again with backoff.
    Unreachable,
}

impl Link {
    /// A short status for a row.
    pub fn label(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting…",
            Self::Live => "Live",
            Self::Polling => "Live (polling)",
            Self::Reconnecting => "Reconnecting…",
            Self::TokenRefused => "Token refused",
            Self::Forbidden => "Network refused",
            Self::Unreachable => "Unreachable",
        }
    }

    /// What the person can do about it, when anything.
    pub fn help(self) -> Option<&'static str> {
        match self {
            Self::TokenRefused => Some(
                "Stargate refused the token. Check it, or create a new one with stargate api token.",
            ),
            Self::Forbidden => Some(
                "Stargate refused this network. Reach it over your private network: Tailscale, a Cloudflare Tunnel, or an SSH tunnel.",
            ),
            Self::Unreachable => Some(
                "Cannot reach this address. Check that the fleet VM is running and your private network is up.",
            ),
            _ => None,
        }
    }
}
