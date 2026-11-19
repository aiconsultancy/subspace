//! TLS configuration using the platform trust store and optional PEM files.

use std::path::Path;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{self, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime, pem::PemObject};
use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
use rustls_platform_verifier::Verifier;
use tokio_rustls::TlsConnector;

use super::ConnectError;
use crate::model::NetworkConfig;

pub(super) async fn prepare(
    config: &NetworkConfig,
) -> Result<(TlsConnector, ServerName<'static>), ConnectError> {
    let name = ServerName::try_from(config.host.clone()).map_err(configuration)?;
    let provider = Arc::new(crypto::aws_lc_rs::default_provider());
    let verifier = server_verifier(config, &provider).await?;
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(configuration)?
        .dangerous()
        .with_custom_certificate_verifier(verifier);

    let client = if let Some(cert_path) = &config.client_cert {
        let chain = certificates(cert_path).await?;
        let key_path = config.client_key.as_deref().unwrap_or(cert_path);
        let bytes = read_pem(key_path).await?;
        let key = PrivateKeyDer::from_pem_slice(&bytes).map_err(|err| pem_error(key_path, err))?;
        builder
            .with_client_auth_cert(chain, key)
            .map_err(configuration)?
    } else {
        builder.with_no_client_auth()
    };
    Ok((TlsConnector::from(Arc::new(client)), name))
}

async fn server_verifier(
    config: &NetworkConfig,
    provider: &Arc<CryptoProvider>,
) -> Result<Arc<dyn ServerCertVerifier>, ConnectError> {
    if config.accept_invalid_certs {
        return Ok(Arc::new(UncheckedCertificate(Arc::clone(provider))));
    }
    let verifier = if let Some(path) = &config.ca_cert {
        let roots = certificates(path).await?;
        Verifier::new_with_extra_roots(roots, Arc::clone(provider))
    } else {
        Verifier::new(Arc::clone(provider))
    };
    Ok(Arc::new(verifier.map_err(configuration)?))
}

async fn certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, ConnectError> {
    let bytes = read_pem(path).await?;
    let chain = CertificateDer::pem_slice_iter(&bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| pem_error(path, err))?;
    if chain.is_empty() {
        return Err(pem_error(path, "no certificates found"));
    }
    Ok(chain)
}

async fn read_pem(path: &Path) -> Result<Vec<u8>, ConnectError> {
    tokio::fs::read(path)
        .await
        .map_err(|err| pem_error(path, err))
}

fn pem_error(path: &Path, error: impl std::fmt::Display) -> ConnectError {
    ConnectError::Configuration(format!("{}: {error}", path.display()))
}

fn configuration(error: impl std::fmt::Display) -> ConnectError {
    ConnectError::Configuration(error.to_string())
}

/// Development-only verifier. The peer must still prove possession of the
/// certificate's private key during the handshake.
#[derive(Debug)]
struct UncheckedCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for UncheckedCertificate {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        crypto::verify_tls12_signature(
            message,
            cert,
            signature,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        crypto::verify_tls13_signature(
            message,
            cert,
            signature,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
