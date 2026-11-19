//! Persistent destination-only CONNECT. No TLS interception or API operation
//! authorization. DNS is resolved and vetted during preparation, never in a
//! tunnel thread, so Stop has no resolver thread to abandon.

mod tunnel;

use crate::net_probe::{Authority, public_ip};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_DESTINATIONS: usize = 32;
const MAX_ADDRESSES: usize = 32;
const MAX_CONNECTIONS: usize = 8;
const CAPACITY_WRITE_TIMEOUT: Duration = Duration::from_millis(100);
const ACCEPT_POLL: Duration = Duration::from_millis(10);

#[derive(Default)]
pub(super) struct DestinationPolicy {
    destinations: BTreeMap<(String, u16), Vec<SocketAddr>>,
}

impl DestinationPolicy {
    pub(super) fn new(destinations: &[String]) -> Result<Self> {
        ensure!(
            destinations.len() <= MAX_DESTINATIONS,
            "too many destination grants"
        );
        let mut policy = Self::default();
        for destination in destinations {
            let authority = origin(destination)?;
            let addresses: Vec<_> = (authority.host.as_str(), authority.port)
                .to_socket_addrs()?
                .take(MAX_ADDRESSES + 1)
                .collect();
            ensure!(
                !addresses.is_empty() && addresses.len() <= MAX_ADDRESSES,
                "invalid destination address count"
            );
            ensure!(
                addresses.iter().all(|address| public_ip(address.ip())),
                "private, loopback and link-local destinations are denied"
            );
            policy
                .destinations
                .insert((authority.host, authority.port), addresses);
        }
        Ok(policy)
    }

    fn addresses(&self, target: &str) -> Result<&[SocketAddr]> {
        let authority = Authority::parse(target)?;
        self.destinations
            .get(&(authority.host, authority.port))
            .map(Vec::as_slice)
            .context("destination is not granted")
    }

    #[cfg(test)]
    pub(in crate::project_runtime) fn local_fixture(address: SocketAddr) -> Self {
        Self {
            destinations: BTreeMap::from([(
                (address.ip().to_string(), address.port()),
                vec![address],
            )]),
        }
    }
}

fn origin(destination: &str) -> Result<Authority> {
    let url = url::Url::parse(destination)?;
    ensure!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "destination must be an HTTPS origin without credentials, path, query or fragment"
    );
    let host = url.host_str().context("destination host missing")?;
    let port = url
        .port_or_known_default()
        .context("destination port missing")?;
    ensure!(port != 0, "destination port must be nonzero");
    Authority::parse(&format!("{host}:{port}"))
}

pub(super) struct PersistentProxy {
    address: SocketAddr,
    cancelled: Arc<AtomicBool>,
    connections: Arc<Mutex<BTreeMap<u64, TcpStream>>>,
    listener: Option<JoinHandle<()>>,
}

impl PersistentProxy {
    pub(super) fn start(policy: DestinationPolicy) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(Mutex::new(BTreeMap::new()));
        let accept = Acceptor {
            cancelled: cancelled.clone(),
            connections: connections.clone(),
            policy: Arc::new(policy),
        };
        let listener = Some(thread::spawn(move || accept.run(listener)));
        Ok(Self {
            address,
            cancelled,
            connections,
            listener,
        })
    }

    pub(super) fn address(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for PersistentProxy {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(connections) = self.connections.lock() {
            for connection in connections.values() {
                let _ = connection.shutdown(Shutdown::Both);
            }
        }
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

#[derive(Clone)]
struct Acceptor {
    cancelled: Arc<AtomicBool>,
    connections: Arc<Mutex<BTreeMap<u64, TcpStream>>>,
    policy: Arc<DestinationPolicy>,
}

impl Acceptor {
    fn run(&self, listener: TcpListener) {
        let mut workers: Vec<JoinHandle<()>> = Vec::new();
        let mut next = 0u64;
        while !self.cancelled.load(Ordering::Acquire) {
            reap(&mut workers);
            match listener.accept() {
                Ok((mut client, _)) if workers.len() < MAX_CONNECTIONS => {
                    next = next.wrapping_add(2);
                    let owner = self.clone();
                    let id = next;
                    workers.push(thread::spawn(move || {
                        let _ = owner.serve(&mut client, id);
                        if let Ok(mut connections) = owner.connections.lock() {
                            connections.remove(&id);
                            connections.remove(&(id + 1));
                        }
                    }));
                }
                Ok((mut client, _)) => {
                    use std::io::Write as _;
                    let _ = client.set_write_timeout(Some(CAPACITY_WRITE_TIMEOUT));
                    let _ = client
                        .write_all(b"HTTP/1.1 503 Capacity exceeded\r\nConnection: close\r\n\r\n");
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(ACCEPT_POLL)
                }
                Err(_) => break,
            }
        }
        for worker in workers {
            let _ = worker.join();
        }
    }

    fn serve(&self, client: &mut TcpStream, id: u64) -> Result<()> {
        self.track(id, client)?;
        let target = tunnel::read_target(client)?;
        let addresses = match self.policy.addresses(&target) {
            Ok(addresses) => addresses,
            Err(error) => {
                tunnel::deny(client);
                return Err(error);
            }
        };
        let upstream = tunnel::connect(addresses, &self.cancelled)?;
        self.track(id + 1, &upstream)?;
        tunnel::relay(client, upstream, &self.cancelled)
    }

    fn track(&self, id: u64, stream: &TcpStream) -> Result<()> {
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| anyhow::anyhow!("proxy connection lock failed"))?;
        ensure!(!self.cancelled.load(Ordering::Acquire), "proxy stopped");
        connections.insert(id, stream.try_clone()?);
        Ok(())
    }
}

fn reap(workers: &mut Vec<JoinHandle<()>>) {
    let mut ix = 0;
    while ix < workers.len() {
        if workers[ix].is_finished() {
            let _ = workers.swap_remove(ix).join();
        } else {
            ix += 1;
        }
    }
}

#[cfg(test)]
mod tests;
