use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{Value, json};

pub(super) const LOGIN: &str = "Alex-J-Example_emu";
pub(super) use subspace_framework::product::fixture::RELEASE_REPOSITORY as REPOSITORY;
pub(super) const BOARD: &str = "subspace-desktop/web-spaces/examples/weather";
pub(super) const TOKEN: &str = "dummy-test-only-nonworking-token";

pub(super) struct Server {
    pub url: String,
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn start(archive: Vec<u8>, denied: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let (stop, receiver) = mpsc::channel();
        let thread = thread::spawn(move || {
            while receiver.recv_timeout(Duration::from_millis(5)).is_err() {
                match listener.accept() {
                    Ok((stream, _)) => respond(stream, &archive, denied),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => panic!("test listener: {error}"),
                }
            }
        });
        Self {
            url,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}

fn request(stream: &TcpStream) -> (String, Value, bool) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut length = 0;
    let mut authorized = false;
    let mut json_accept = false;
    loop {
        let mut header = String::new();
        assert_ne!(reader.read_line(&mut header).unwrap(), 0);
        if header == "\r\n" {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            length = value.trim().parse().unwrap();
        }
        if lower.starts_with("authorization:") {
            authorized = header.contains(TOKEN);
        }
        if lower.starts_with("accept:") {
            json_accept = lower.contains("application/vnd.github+json");
        }
    }
    assert!(authorized, "Only the synthetic token is permitted");
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).unwrap();
    (
        line,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        json_accept,
    )
}

fn respond(mut stream: TcpStream, archive: &[u8], denied: bool) {
    let (line, body, json_accept) = request(&stream);
    let (status, bytes) = if line.contains("/zipball/") && !json_accept {
        (415, b"raw-test-only-wrong-accept-body".to_vec())
    } else {
        response(&line, &body, archive, denied)
    };
    write!(
        stream,
        "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    )
    .unwrap();
    stream.write_all(&bytes).unwrap();
}

fn response(line: &str, body: &Value, archive: &[u8], denied: bool) -> (u16, Vec<u8>) {
    let (organisation, _) = REPOSITORY.split_once('/').unwrap();
    let value = if body["query"] == "query ViewerBound { viewer { id login } }" {
        json!({"data":{"viewer":{"id":"test-only-viewer-id","login":LOGIN}}})
    } else if line.starts_with(&format!("GET /user/memberships/orgs/{organisation} ")) {
        if denied {
            return (403, b"raw-test-only-body-must-not-escape".to_vec());
        }
        json!({"state":"active","organization":{"login":organisation}})
    } else if line.starts_with(&format!("GET /repos/{REPOSITORY} ")) {
        json!({"full_name":REPOSITORY,"default_branch":"main","owner":{"type":"Organization"},"permissions":{"pull":true}})
    } else if line.starts_with(&format!("GET /repos/{REPOSITORY}/commits/")) {
        assert!(line.contains("/commits/refs%2Fheads%2Fmain "), "{line}");
        json!({"sha":"b".repeat(40)})
    } else {
        assert!(
            line.starts_with(&format!(
                "GET /repos/{REPOSITORY}/zipball/{} ",
                "b".repeat(40)
            )),
            "{line}"
        );
        return (200, archive.to_vec());
    };
    (200, serde_json::to_vec(&value).unwrap())
}

pub(super) fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut output = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut output);
    for (name, bytes) in files {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
    output.into_inner()
}
