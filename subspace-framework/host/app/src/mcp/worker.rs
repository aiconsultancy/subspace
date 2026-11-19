//! The app server's Tokio side. One task owns the host and runs commands in
//! order, so a stop always finishes before the next listener binds.

use std::net::SocketAddr;

use subspace_mcp::{BindError, Host, HostConfig, ServedCatalog};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{mpsc, oneshot};

const WORKER_THREADS: usize = 2;
const THREAD_NAME: &str = "subspace-mcp";

pub(super) type StartReply = oneshot::Receiver<Result<SocketAddr, String>>;

pub(super) enum Command {
    Start {
        config: HostConfig,
        catalog: ServedCatalog,
        reply: oneshot::Sender<Result<SocketAddr, String>>,
    },
    Swap(ServedCatalog),
    Stop(Option<oneshot::Sender<()>>),
}

/// The runtime and the command queue of its host task.
pub(super) struct Worker {
    commands: mpsc::UnboundedSender<Command>,
    _runtime: Runtime,
}

impl Worker {
    pub(super) fn start() -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(WORKER_THREADS)
            .thread_name(THREAD_NAME)
            .enable_all()
            .build()
            .map_err(|error| format!("cannot start the MCP runtime: {error}"))?;
        let (commands, queue) = mpsc::unbounded_channel();
        runtime.spawn(run(queue, runtime.handle().clone()));
        Ok(Self {
            commands,
            _runtime: runtime,
        })
    }

    /// A worker whose commands wait in the returned queue and never run, so
    /// a test decides when a start replies.
    #[cfg(test)]
    pub(super) fn held() -> std::io::Result<(Self, mpsc::UnboundedReceiver<Command>)> {
        let runtime = tokio::runtime::Builder::new_current_thread().build()?;
        let (commands, queue) = mpsc::unbounded_channel();
        let worker = Self {
            commands,
            _runtime: runtime,
        };
        Ok((worker, queue))
    }

    pub(super) fn start_host(&self, config: HostConfig, catalog: ServedCatalog) -> StartReply {
        let (reply, receiver) = oneshot::channel();
        self.send(Command::Start {
            config,
            catalog,
            reply,
        });
        receiver
    }

    pub(super) fn swap(&self, catalog: ServedCatalog) {
        self.send(Command::Swap(catalog));
    }

    /// Closes the listener before any later command runs.
    pub(super) fn stop(&self) {
        self.send(Command::Stop(None));
    }

    /// Resolves when the listener is closed.
    pub(super) fn stopped(&self) -> oneshot::Receiver<()> {
        let (reply, receiver) = oneshot::channel();
        self.send(Command::Stop(Some(reply)));
        receiver
    }

    fn send(&self, command: Command) {
        // The task lives as long as the runtime; a closed queue means shutdown.
        _ = self.commands.send(command);
    }
}

async fn run(mut queue: mpsc::UnboundedReceiver<Command>, runtime: Handle) {
    let mut host = None;
    while let Some(command) = queue.recv().await {
        apply(&mut host, command, &runtime).await;
    }
    stop(&mut host).await;
}

async fn apply(host: &mut Option<Host>, command: Command, runtime: &Handle) {
    match command {
        Command::Start {
            config,
            catalog,
            reply,
        } => {
            stop(host).await;
            _ = reply.send(start(host, config, catalog, runtime).await);
        }
        Command::Swap(catalog) => {
            if let Some(host) = host {
                host.replace_catalog(catalog);
            }
        }
        Command::Stop(reply) => {
            stop(host).await;
            if let Some(reply) = reply {
                _ = reply.send(());
            }
        }
    }
}

async fn start(
    host: &mut Option<Host>,
    config: HostConfig,
    catalog: ServedCatalog,
    runtime: &Handle,
) -> Result<SocketAddr, String> {
    let started = Host::start(runtime, config, catalog)
        .await
        .map_err(bind_reason)?;
    let address = started.address();
    *host = Some(started);
    Ok(address)
}

/// Names an address in use the same way on every platform (REQ-15).
fn bind_reason(error: BindError) -> String {
    match error {
        BindError::AddrInUse(message) => format!("the address is in use. {message}"),
        BindError::Other(message) => message,
    }
}

async fn stop(host: &mut Option<Host>) {
    if let Some(mut running) = host.take() {
        running.stop().await;
    }
}

#[cfg(test)]
mod tests {
    use std::net::{SocketAddr, TcpListener};

    use subspace_agent_tools::Catalog;
    use subspace_mcp::{HostConfig, HttpOptions, Identity};

    use super::Worker;

    fn config(bind: SocketAddr) -> HostConfig {
        let http = HttpOptions {
            bind,
            token: None,
            origins: Vec::new(),
            hosts: Vec::new(),
        };
        HostConfig::new(http, Identity::new("subspace", "Subspace", "0.0.0"))
    }

    fn catalog() -> subspace_mcp::ServedCatalog {
        Catalog::new(Vec::new()).unwrap().into()
    }

    #[test]
    fn a_stop_closes_the_listener_and_a_used_port_reports_in_use() {
        let waiter = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let worker = Worker::start().unwrap();
        let address = waiter
            .block_on(worker.start_host(config(([127, 0, 0, 1], 0).into()), catalog()))
            .unwrap()
            .unwrap();
        assert!(TcpListener::bind(address).is_err(), "the server listens");
        waiter.block_on(worker.stopped()).unwrap();
        assert!(frees(address), "a stop closes the listener");

        // A port this test holds itself: the freed one may already belong to
        // another test's listener.
        let held = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let used = held.local_addr().unwrap();
        let error = waiter
            .block_on(worker.start_host(config(used), catalog()))
            .unwrap()
            .unwrap_err();
        assert!(error.starts_with("the address is in use."), "{error}");
    }

    /// Whether `address` can be bound again within two seconds. The system
    /// may release a closed listener's port a moment after the stop returns.
    fn frees(address: SocketAddr) -> bool {
        for _ in 0..40 {
            if TcpListener::bind(address).is_ok() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }
}
