//! Loopback REST and SSE responses. Tests control stream events and EOF.
use super::UPDATES;
use std::{
    io::{BufRead as _, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    time::Duration,
};

#[derive(Clone, Debug)]
pub(crate) struct Seen {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
}

impl Seen {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

type Handler = dyn Fn(&Seen) -> (u16, String) + Send + Sync;

pub(crate) struct Server {
    pub base: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    peers: Receiver<Peer>,
    stop: Arc<AtomicBool>,
}

impl Server {
    pub(crate) fn new(handler: impl Fn(&Seen) -> (u16, String) + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (sender, peers) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let handler: Arc<Handler> = Arc::new(handler);
        let log = seen.clone();
        let stopped = stop.clone();
        std::thread::spawn(move || accept(listener, handler, log, sender, stopped));
        Self {
            base,
            seen,
            peers,
            stop,
        }
    }

    pub(crate) fn count(&self, method: &str, path: &str) -> usize {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|seen| seen.method == method && route(&seen.path) == path)
            .count()
    }

    pub(crate) fn count_if(&self, keep: impl Fn(&Seen) -> bool) -> usize {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|seen| keep(seen))
            .count()
    }

    pub(crate) fn peer(&self) -> Peer {
        self.peers
            // A reconnection can wait out the hub's backoff, several
            // seconds, before a busy machine adds its own delay.
            .recv_timeout(Duration::from_secs(15))
            .expect("no SSE stream connected")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

pub(super) fn route(path: &str) -> &str {
    path.split('?').next().unwrap()
}

fn accept(
    listener: TcpListener,
    handler: Arc<Handler>,
    log: Arc<Mutex<Vec<Seen>>>,
    peers: Sender<Peer>,
    stopped: Arc<AtomicBool>,
) {
    while !stopped.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let (handler, log, sender) = (handler.clone(), log.clone(), peers.clone());
                std::thread::spawn(move || serve(stream, &*handler, &log, &sender));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("fixture accept failed: {error}"),
        }
    }
}

fn serve(mut stream: TcpStream, handler: &Handler, log: &Mutex<Vec<Seen>>, peers: &Sender<Peer>) {
    // Some platforms inherit the listening socket's nonblocking mode.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let Some(seen) = read_head(&mut reader) else {
        return;
    };
    log.lock().unwrap().push(seen.clone());
    let (status, body) = handler(&seen);
    if route(&seen.path) == UPDATES && status == 200 && body.is_empty() {
        let head = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
        if stream.write_all(head).is_ok() {
            let _ = peers.send(Peer { stream, seen });
        }
        return;
    }
    respond(&mut stream, status, &body);
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    match status {
        0 => {} // Disconnect before sending any response.
        302 => {
            let _ = write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: {body}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
        _ => {
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    }
}

fn read_head(reader: &mut BufReader<TcpStream>) -> Option<Seen> {
    let mut first = String::new();
    if reader.read_line(&mut first).ok()? == 0 {
        return None;
    }
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.push((key.trim().to_lowercase(), value.trim().to_owned()));
        }
    }
    let mut parts = first.split_whitespace();
    let seen = Seen {
        method: parts.next()?.into(),
        path: parts.next()?.into(),
        headers,
    };
    let size = seen
        .header("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    reader.read_exact(&mut vec![0; size]).ok()?;
    Some(seen)
}

pub(crate) struct Peer {
    stream: TcpStream,
    pub seen: Seen,
}

impl Peer {
    pub(crate) fn chunk(&mut self, text: &[u8]) {
        write!(self.stream, "{:x}\r\n", text.len()).unwrap();
        self.stream.write_all(text).unwrap();
        self.stream.write_all(b"\r\n").unwrap();
    }

    pub(crate) fn ready(&mut self) {
        self.chunk(b"event: ready\ndata: null\n\n");
    }

    pub(crate) fn close(mut self) {
        self.chunk(b"");
    }

    pub(crate) fn changed(&mut self, identifier: &str, version: i64) {
        self.hint(identifier, version, "issue.changed");
    }

    pub(crate) fn hint(&mut self, identifier: &str, version: i64, action: &str) {
        let data = payload(identifier, version, action);
        self.chunk(format!("event: issueChanged\ndata: {data}\n\n").as_bytes());
    }
}

pub(crate) fn payload(identifier: &str, version: i64, action: &str) -> String {
    let id = identifier.trim_start_matches("SUB-");
    serde_json::json!({
        "projectKey":"SUB","issueId":id,"identifier":identifier,"parentIdentifier":null,
        "version":version,"auditId":"00000000-0000-0000-0000-0000000000a1","action":action,
        "actorType":"human","occurredAt":"2026-11-17T10:00:00Z"
    })
    .to_string()
}
