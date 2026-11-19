//! App-owned HTTP hosting; the caller owns the Tokio runtime.
use std::net::SocketAddr;

use tokio::runtime::Handle;
use tokio::sync::oneshot;

use crate::{BindError, Endpoint, HttpOptions, Identity, ServedCatalog, ToolServer};

#[non_exhaustive]
pub struct HostConfig {
    pub http: HttpOptions,
    pub identity: Identity,
}

impl HostConfig {
    pub fn new(http: HttpOptions, identity: Identity) -> Self {
        Self { http, identity }
    }
}

pub struct Host {
    address: SocketAddr,
    server: ToolServer,
    stop: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Host {
    /// Bind on `runtime`, even when called outside an entered Tokio context.
    pub async fn start(
        runtime: &Handle,
        config: HostConfig,
        catalog: impl Into<ServedCatalog>,
    ) -> Result<Self, BindError> {
        let server = ToolServer::new(catalog, config.identity);
        let binding = server.clone();
        let endpoint = runtime
            .spawn(async move { Endpoint::bind(binding, &config.http).await })
            .await
            .map_err(|error| BindError::Other(format!("Cannot start MCP host: {error}")))??;
        let address = endpoint.address().map_err(BindError::Other)?;
        let (stop, stopped) = oneshot::channel();
        let task = runtime.spawn(endpoint.serve(async {
            let _ = stopped.await;
        }));
        Ok(Self {
            address,
            server,
            stop: Some(stop),
            task: Some(task),
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn replace_catalog(&self, catalog: impl Into<ServedCatalog>) {
        self.server.replace_catalog(catalog);
    }

    /// Waits until the old listener is closed before a caller binds a replacement.
    pub async fn stop(&mut self) {
        self.signal_stop();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }

    fn signal_stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.signal_stop();
    }
}
