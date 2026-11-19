use std::{
    io::{Read, Seek, Write},
    path::Path,
    sync::Arc,
    time::Duration,
};

use super::{
    manifest::{Asset, ORIGIN},
    staging::Stage,
    verify_reader,
};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        // Follow the application's transport proxy policy, never HF model/token variables.
        .proxy(ureq::Proxy::try_from_env())
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .unversioned_rustls_crypto_provider(Arc::new(
                    rustls::crypto::aws_lc_rs::default_provider(),
                ))
                .build(),
        )
        .build()
        .into()
}

pub(super) fn asset(agent: &ureq::Agent, root: &Path, asset: &Asset) -> Result<(), String> {
    let mut response = agent
        .get(format!("{ORIGIN}/{}", asset.remote))
        .call()
        .map_err(|_| "Public model download failed; check connectivity and retry installation.")?;
    let mut stage = Stage::create(root, asset.name)?;
    let mut reader = response.body_mut().as_reader().take(asset.bytes + 1);
    let copied = std::io::copy(&mut reader, &mut stage.file)
        .map_err(|_| "Model download was interrupted.")?;
    if copied != asset.bytes {
        return Err("Downloaded model asset has an unexpected size.".into());
    }
    stage
        .file
        .flush()
        .map_err(|_| "Cannot flush model asset.")?;
    stage
        .file
        .rewind()
        .map_err(|_| "Cannot verify model asset.")?;
    verify_reader(&mut stage.file, asset)?;
    stage.commit(&root.join(asset.name))
}
