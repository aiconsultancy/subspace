//! Opens the transport for an IRC network.

mod tls;

use std::{io, time::Duration};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::model::NetworkConfig;

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
pub type BoxStream = Box<dyn Stream>;

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub tcp: Duration,
    pub tls: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            tcp: Duration::from_secs(15),
            tls: Duration::from_secs(15),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("{step} timed out after {secs} seconds")]
    Timeout { step: &'static str, secs: u64 },
    #[error("TLS: {0}")]
    Handshake(#[source] io::Error),
    #[error("TLS: {0}")]
    Configuration(String),
}

impl ConnectError {
    pub fn retryable(&self) -> bool {
        !matches!(self, Self::Configuration(_))
    }
}

pub async fn connect(
    config: &NetworkConfig,
    timeouts: Timeouts,
) -> Result<BoxStream, ConnectError> {
    let tcp = timeout(
        timeouts.tcp,
        TcpStream::connect((config.host.as_str(), config.port)),
    )
    .await
    .map_err(|_| ConnectError::Timeout {
        step: "Connecting",
        secs: timeouts.tcp.as_secs(),
    })??;
    tcp.set_nodelay(true)?;
    if !config.tls {
        return Ok(Box::new(tcp));
    }
    let (connector, name) = tls::prepare(config).await?;
    let stream = timeout(timeouts.tls, connector.connect(name, tcp))
        .await
        .map_err(|_| ConnectError::Timeout {
            step: "The TLS handshake",
            secs: timeouts.tls.as_secs(),
        })?
        .map_err(ConnectError::Handshake)?;
    Ok(Box::new(stream))
}
