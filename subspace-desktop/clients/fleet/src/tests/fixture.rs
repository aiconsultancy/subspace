//! The loopback Stargate Fleet API fixture, `scripts/fleet_api_test_server.py`,
//! started for one test and stopped when dropped.

use std::io::{BufRead as _, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use subspace_framework::core::Secret;

use crate::{FleetAddress, FleetClient};

/// The fixture's fictional token.
pub(crate) const TOKEN: &str = "fixture-fleet";
const BANNER: &str = "Stargate Fleet API fixture on ";

pub(crate) struct FleetFixture {
    child: Child,
    pub(crate) origin: String,
}

impl FleetFixture {
    pub(crate) fn start(extra: &[&str]) -> Self {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/fleet_api_test_server.py");
        let python = subspace_framework::core::path::find_tool("python3")
            .expect("python3 runs the Fleet API fixture");
        let mut child = Command::new(python)
            .arg("-I")
            .arg(&script)
            .args(["--port", "0", "--token", TOKEN, "--tick", "0.3"])
            .args(extra)
            .env_clear()
            // Windows Python needs SYSTEMROOT to open a socket.
            .envs(std::env::var_os("SYSTEMROOT").map(|root| ("SYSTEMROOT", root)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("python3 runs the Fleet API fixture");
        let origin = banner(&mut child).expect("the Fleet API fixture started");
        Self { child, origin }
    }

    pub(crate) fn address(&self) -> FleetAddress {
        FleetAddress::parse(&self.origin).expect("the fixture is on loopback")
    }

    pub(crate) fn client(&self, token: &str) -> FleetClient {
        FleetClient::new(self.address(), Secret::new(token))
    }
}

impl Drop for FleetFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn banner(child: &mut Child) -> Result<String, &'static str> {
    let stdout = child.stdout.take().ok_or("no fixture stdout")?;
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = send.send(BufReader::new(stdout).read_line(&mut line).map(|_| line));
    });
    // A machine busy with other builds can take far longer than usual to
    // start Python; the limit only bounds a fixture that never starts.
    let line = receive
        .recv_timeout(Duration::from_secs(60))
        .map_err(|_| "fixture startup timed out")?
        .map_err(|_| "could not read the fixture banner")?;
    line.trim()
        .strip_prefix(BANNER)
        .map(str::to_owned)
        .ok_or("unexpected fixture banner")
}

/// Waits up to fifteen seconds for `condition`.
pub(crate) fn eventually(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    condition()
}

/// A loopback address nothing listens on.
pub(crate) fn closed_address() -> FleetAddress {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free loopback port");
    let port = listener.local_addr().expect("its address").port();
    drop(listener);
    FleetAddress::parse(&format!("http://127.0.0.1:{port}")).expect("a loopback address")
}
