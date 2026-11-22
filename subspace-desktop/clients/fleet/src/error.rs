//! Why a Fleet API call failed. No variant carries an address, a token or
//! response text, so an error can be shown or logged as it is.

/// A failed Fleet API call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error(
        "Use an HTTPS address, or HTTP on this computer, without a user name, query or fragment."
    )]
    InvalidAddress,
    #[error(
        "Stargate refused the token (401). Check it, or create a new one with stargate api token."
    )]
    TokenRefused,
    #[error(
        "Stargate refused this network (403). Reach it over your private network: Tailscale, a Cloudflare Tunnel, or an SSH tunnel."
    )]
    Forbidden,
    #[error(
        "No Fleet API at this address (404). Check the address and that Stargate's api block is enabled."
    )]
    NotFound,
    /// 429 or 503, with the server's wait in whole seconds when it gave one.
    #[error("Stargate is busy. Subspace will try again.")]
    Busy(Option<u64>),
    #[error("Stargate answered {0}. Check the fleet VM and Stargate's logs.")]
    UnexpectedStatus(u16),
    #[error(
        "Cannot reach this address. Check that the fleet VM is running and your private network is up."
    )]
    Unreachable,
    #[error("Stargate sent an invalid or oversized response.")]
    InvalidResponse,
}
