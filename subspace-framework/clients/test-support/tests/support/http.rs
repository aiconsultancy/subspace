//! Local HTTP transport shared only by feature-gated workflow test fixtures.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

#[derive(Clone, Default)]
#[non_exhaustive]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: String,
    pub bytes: Vec<u8>,
    pub headers: Vec<(String, String)>,
}
pub struct Mock {
    base: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Mock {
    pub fn new(handler: impl Fn(&Request) -> (u16, String) + Send + Sync + 'static) -> Self {
        Self::with_headers(move |request| {
            let (status, body) = handler(request);
            (status, Vec::new(), body)
        })
    }
    pub fn with_headers(
        handler: impl Fn(&Request) -> (u16, Vec<(String, String)>, String) + Send + Sync + 'static,
    ) -> Self {
        Self::with_bytes(move |request| {
            let (status, headers, body) = handler(request);
            (status, headers, body.into_bytes())
        })
    }
    pub fn with_bytes(
        handler: impl Fn(&Request) -> (u16, Vec<(String, String)>, Vec<u8>) + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let seen = requests.clone();
        let stopping = stop.clone();
        let handler = Arc::new(handler);
        let thread = std::thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let h = handler.clone();
                        let seen = seen.clone();
                        std::thread::spawn(move || respond(stream, h.as_ref(), &seen));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base: format!("http://{address}"),
            requests,
            stop,
            thread: Some(thread),
        }
    }
    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
    pub fn base_url(&self) -> &str {
        &self.base
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
fn respond(
    mut stream: TcpStream,
    handler: &(impl Fn(&Request) -> (u16, Vec<(String, String)>, Vec<u8>) + ?Sized),
    seen: &Mutex<Vec<Request>>,
) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    let parts = first.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 {
        return;
    }
    let mut len = 0;
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
                len = value.trim().parse().unwrap();
            }
            headers.push((key.to_lowercase(), value.trim().to_owned()));
        }
    }
    let mut body = vec![0; len];
    reader.read_exact(&mut body).unwrap();
    let request = Request {
        method: parts[0].into(),
        path: parts[1].into(),
        body: String::from_utf8_lossy(&body).into_owned(),
        bytes: body,
        headers,
    };
    seen.lock().unwrap().push(request.clone());
    let (status, headers, body) = handler(&request);
    // A zero status simulates a transport disconnect, without an HTTP response.
    if status == 0 {
        return;
    }
    write!(
        stream,
        "HTTP/1.1 {status} response\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    )
    .unwrap();
    if !headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case("content-type"))
    {
        write!(stream, "Content-Type: application/json\r\n").unwrap();
    }
    for (key, value) in headers {
        write!(stream, "{key}: {value}\r\n").unwrap();
    }
    write!(stream, "\r\n").unwrap();
    stream.write_all(&body).unwrap();
}
