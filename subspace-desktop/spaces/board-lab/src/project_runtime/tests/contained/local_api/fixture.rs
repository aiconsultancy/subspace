use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(super) const API_SCOPE: &str = "synthetic-local-api";
pub(super) const API_EVALUATED_AT_MS: u64 = 1_700_000_000_123;
const RESPONSE: &[u8] = br#"{"scope":"synthetic-local-api","evaluated_at_ms":1700000000123,"version":"1.2.3","count":17}"#;
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const IO_POLL: Duration = Duration::from_millis(100);
const ACCEPT_POLL: Duration = Duration::from_millis(10);
const HEADER_BYTES: usize = 8192;

pub(super) struct Observation {
    pub(super) target: String,
    pub(super) authorization: String,
}

pub(super) struct ApiFixture {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    observed: mpsc::Receiver<io::Result<Observation>>,
    worker: Option<JoinHandle<()>>,
}

impl ApiFixture {
    pub(super) fn start() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let (sender, observed) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let _ = sender.send(serve(listener, &server_stop));
        });
        Ok(Self {
            address,
            stop,
            observed,
            worker: Some(worker),
        })
    }

    pub(super) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(super) fn observation(&self) -> io::Result<Observation> {
        self.observed.recv_timeout(READ_TIMEOUT).map_err(|_| {
            io::Error::new(io::ErrorKind::TimedOut, "host API observation unavailable")
        })?
    }
}

impl Drop for ApiFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(listener: TcpListener, stop: &AtomicBool) -> io::Result<Observation> {
    let deadline = Instant::now() + ACCEPT_TIMEOUT;
    while Instant::now() < deadline && !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((client, _)) => return request(client),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "synthetic API received no request before stop or deadline",
    ))
}

fn request(mut client: TcpStream) -> io::Result<Observation> {
    client.set_read_timeout(Some(IO_POLL))?;
    client.set_write_timeout(Some(IO_POLL))?;
    let header = read_header(&mut client)?;
    let target = header.lines().next().unwrap_or_default().to_owned();
    let authorization = header
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("Authorization"))
        .map(|(_, value)| value.trim().to_owned())
        .unwrap_or_default();
    let valid = target == "GET /metrics HTTP/1.1"
        && authorization == "Bearer dummy-test-only-selected-value";
    let status = if valid { "200 OK" } else { "401 Unauthorized" };
    write!(
        client,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        RESPONSE.len()
    )?;
    client.write_all(RESPONSE)?;
    Ok(Observation {
        target,
        authorization,
    })
}

fn read_header(client: &mut TcpStream) -> io::Result<String> {
    let deadline = Instant::now() + READ_TIMEOUT;
    let mut header = Vec::new();
    while header.len() < HEADER_BYTES && Instant::now() < deadline {
        let mut byte = [0u8; 1];
        match client.read(&mut byte) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "API header closed",
                ));
            }
            Ok(_) => header.push(byte[0]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
        if header.ends_with(b"\r\n\r\n") {
            return String::from_utf8(header).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "API header is not UTF-8")
            });
        }
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "API header exceeds fixture size or time budget",
    ))
}
