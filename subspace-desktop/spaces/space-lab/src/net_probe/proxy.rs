use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::io::ErrorKind;
use std::net::TcpListener;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::authority::{Authority, public_port};
use super::serve::serve;

const PROXY_TTL: Duration = Duration::from_secs(90);
const MAX_PROXY_REQUESTS: usize = 32;
const ACCEPT_POLL: Duration = Duration::from_millis(10);
const MAX_DESTINATIONS: usize = 8;

#[derive(Clone)]
struct ProxyStats {
    allowed: Arc<AtomicUsize>,
    denied: Arc<AtomicUsize>,
}

impl ProxyStats {
    fn new() -> Self {
        Self {
            allowed: Arc::new(AtomicUsize::new(0)),
            denied: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn snapshot(&self) -> Value {
        json!({"allowed":self.allowed.load(Ordering::Relaxed),"denied":self.denied.load(Ordering::Relaxed)})
    }
}

struct ProxyShutdown {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for ProxyShutdown {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) struct PolicyProxy {
    address: String,
    stats: ProxyStats,
    _shutdown: ProxyShutdown,
}

impl PolicyProxy {
    pub(super) fn start(destinations: &[String]) -> Result<Self> {
        Self::start_inner(destinations, false)
    }

    pub(super) fn start_inner(destinations: &[String], local_fixture: bool) -> Result<Self> {
        let rules = parse_destinations(destinations, local_fixture)?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = format!("http://{}", listener.local_addr()?);
        listener.set_nonblocking(true)?;
        let stats = ProxyStats::new();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = spawn_accept(listener, rules, local_fixture, stop.clone(), stats.clone());
        Ok(Self {
            address,
            stats,
            _shutdown: ProxyShutdown {
                stop,
                thread: Some(thread),
            },
        })
    }

    pub(super) fn with_address<T>(
        self,
        work: impl FnOnce(&str) -> Result<T>,
    ) -> Result<(T, Value)> {
        let output = work(&self.address)?;
        Ok((output, self.stats.snapshot()))
    }
}

fn parse_destinations(destinations: &[String], local_fixture: bool) -> Result<Vec<Authority>> {
    ensure!(
        !destinations.is_empty() && destinations.len() <= MAX_DESTINATIONS,
        "Proxy needs 1–8 explicit destinations"
    );
    let rules = destinations
        .iter()
        .map(|value| Authority::parse(value))
        .collect::<Result<Vec<_>>>()?;
    if !local_fixture {
        ensure!(
            rules.iter().all(|rule| public_port(rule.port)),
            "Public prototype allows only ports 80 and 443"
        );
    }
    Ok(rules)
}

fn spawn_accept(
    listener: TcpListener,
    rules: Vec<Authority>,
    local_fixture: bool,
    stop: Arc<AtomicBool>,
    stats: ProxyStats,
) -> JoinHandle<()> {
    thread::spawn(move || accept_loop(listener, rules, local_fixture, stop, stats))
}

fn accept_loop(
    listener: TcpListener,
    rules: Vec<Authority>,
    local_fixture: bool,
    stop: Arc<AtomicBool>,
    stats: ProxyStats,
) {
    let expires = Instant::now() + PROXY_TTL;
    let mut requests = 0;
    while proxy_active(&stop, expires, requests) {
        match listener.accept() {
            Ok((stream, peer)) => {
                requests += 1;
                if peer.ip().is_loopback() {
                    dispatch(stream, rules.clone(), local_fixture, stats.clone());
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
            Err(_) => break,
        }
    }
}

fn proxy_active(stop: &AtomicBool, expires: Instant, requests: usize) -> bool {
    !stop.load(Ordering::Relaxed) && Instant::now() < expires && requests < MAX_PROXY_REQUESTS
}

fn dispatch(
    stream: std::net::TcpStream,
    rules: Vec<Authority>,
    local_fixture: bool,
    stats: ProxyStats,
) {
    thread::spawn(move || {
        let _ = serve(stream, &rules, local_fixture, &stats.allowed, &stats.denied);
    });
}
