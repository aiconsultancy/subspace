//! One process-wide TLS crypto provider. The app's own clients name
//! aws-lc-rs, but `ureq` and `reqwest` built with `rustls-no-provider` use
//! rustls's process default. rustls picks that default itself only when one
//! provider is compiled in; nsh, in a build with TSX Spaces, also turns on
//! ring, and then any client config built without a provider panics.
//! Installing aws-lc-rs at start-up keeps every build on one provider.

/// Installs aws-lc-rs as the process default, unless one is installed.
pub(crate) fn install_provider() {
    // An installed default, such as from an earlier call, stays.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use super::install_provider;

    #[test]
    fn a_client_config_without_a_named_provider_builds() {
        install_provider();
        install_provider();
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();
        assert!(!config.crypto_provider().cipher_suites.is_empty());
    }
}
