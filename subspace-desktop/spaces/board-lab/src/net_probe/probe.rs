use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use url::Url;

use crate::environments::EnvironmentManager;

use super::io::{read_head, response};
use super::proxy::PolicyProxy;
use super::script::fixture_script;

const ACCEPT_POLL: Duration = Duration::from_millis(10);

pub(super) fn run(root: &Path) -> Result<Value> {
    let root = root.to_owned();
    let manager = EnvironmentManager::new(root.join("environments"));
    let environment = manager
        .catalog()?
        .into_iter()
        .find(|info| info.spec.id == "python-data" && info.ready)
        .context("Provision the python-data environment before running the network probe")?;
    let origin = OriginFixture::start()?;
    let result = probe_with_origin(&manager, &environment.project, &origin);
    origin.stop();
    result
}

struct OriginFixture {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl OriginFixture {
    fn start() -> Result<Self> {
        let origin = TcpListener::bind("127.0.0.1:0")?;
        origin.set_nonblocking(true)?;
        let address = origin.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let thread = thread::spawn(move || origin_loop(origin, signal));
        Ok(Self {
            address,
            stop,
            thread,
        })
    }

    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.thread.join();
    }
}

fn origin_loop(origin: TcpListener, signal: Arc<AtomicBool>) {
    while !signal.load(Ordering::Relaxed) {
        match origin.accept() {
            Ok((mut stream, _)) => {
                if read_head(&mut stream).is_ok() {
                    let _ = response(
                        &mut stream,
                        "200 OK",
                        r#"{"fixture":true,"rows":[{"service":"compute","amount":42}]}"#,
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL)
            }
            Err(_) => break,
        }
    }
}

fn probe_with_origin(
    manager: &EnvironmentManager,
    project: &Path,
    origin: &OriginFixture,
) -> Result<Value> {
    confirm_baseline(origin.address)?;
    let proxy = PolicyProxy::start_inner(&[origin.address.to_string()], true)?;
    let (probe, counts) = proxy
        .with_address(|address| run_script(manager, project, address, origin.address.port()))?;
    Ok(
        json!({"probe":probe,"proxy_requests":counts,"note":"Local HTTP and CONNECT fixture only. No live endpoint, credentials or TLS verification bypass used."}),
    )
}

fn confirm_baseline(address: SocketAddr) -> Result<()> {
    let mut baseline = TcpStream::connect(address)?;
    baseline.write_all(b"GET /data HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n")?;
    let mut answer = String::new();
    baseline.read_to_string(&mut answer)?;
    ensure!(answer.contains("200 OK"), "Origin baseline failed");
    Ok(())
}

fn run_script(
    manager: &EnvironmentManager,
    project: &Path,
    proxy_address: &str,
    origin_port: u16,
) -> Result<Value> {
    let proxy_url = Url::parse(proxy_address)?;
    let script = fixture_script(proxy_url.port().context("Proxy has no port")?, origin_port);
    let workspace = project.join("network-workspace");
    fs::create_dir_all(&workspace)?;
    let output =
        manager.run_source_with_proxy("python-data", &script, &json!({}), Some(proxy_address))?;
    Ok(serde_json::from_str::<Value>(output.trim())?)
}
