//! Proof Key for Code Exchange (RFC 7636, S256) and the `state` value.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest as _, Sha256};
use subspace_core::Secret;

/// Random bytes behind a verifier: 32 give the 43-character minimum.
const VERIFIER_BYTES: usize = 32;
const STATE_BYTES: usize = 16;

/// A code verifier and its S256 challenge.
pub struct Pkce {
    pub verifier: Secret,
    pub challenge: String,
}

impl Pkce {
    pub fn generate() -> Self {
        let bytes: [u8; VERIFIER_BYTES] = rand::random();
        Self::from_verifier(&URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn from_verifier(verifier: &str) -> Self {
        Self {
            verifier: Secret::new(verifier),
            challenge: challenge(verifier),
        }
    }
}

/// `BASE64URL(SHA256(verifier))` without padding.
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// A random `state`, unguessable by another page.
pub fn state() -> String {
    let bytes: [u8; STATE_BYTES] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::{Pkce, challenge, state};

    #[test]
    fn the_rfc_7636_example_gives_its_challenge() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generated_values_are_url_safe_long_enough_and_fresh() {
        let first = Pkce::generate();
        let second = Pkce::generate();
        let verifier = first.verifier.expose();
        assert_eq!(verifier.len(), 43);
        assert!(
            verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_eq!(first.challenge, challenge(verifier));
        assert_ne!(first.verifier, second.verifier);
        assert_ne!(state(), state());
        assert_eq!(state().len(), 22);
    }
}
