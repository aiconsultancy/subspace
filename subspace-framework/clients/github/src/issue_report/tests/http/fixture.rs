//! Loopback-only HTTP fixture. Credentials are readable, nonworking test data.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use subspace_core::credentials::{Environment, Runner};

pub(super) struct Request {
    pub line: String,
    pub authorization: String,
    pub body: Value,
}

pub(super) fn capture(responses: Vec<(u16, String)>) -> (String, Receiver<Request>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let request = read_request(&stream);
            sender.send(request).unwrap();
            if status != 0 {
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        }
    });
    (url, receiver)
}

fn read_request(stream: &TcpStream) -> Request {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let (length, authorization) = read_headers(&mut reader);
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    Request {
        line: line.trim_end().into(),
        authorization,
        body,
    }
}

fn read_headers(reader: &mut impl BufRead) -> (usize, String) {
    let (mut length, mut authorization) = (0, String::new());
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.trim_end().split_once(": ") {
            match name.to_ascii_lowercase().as_str() {
                "content-length" => length = value.parse().unwrap(),
                "authorization" => authorization = value.into(),
                _ => {}
            }
        }
    }
    (length, authorization)
}

pub(super) struct NoAuth;

impl Environment for NoAuth {
    fn var(&self, name: &str) -> Option<String> {
        // Avoid inspecting the developer's real gh account file.
        (name == "GH_CONFIG_DIR").then(|| "/test-only/absent-gh-config".into())
    }
}

pub(super) struct NoCli;

impl Runner for NoCli {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        panic!("This fixture must not run a real CLI command")
    }
}

pub(super) struct GhHome(std::path::PathBuf);

impl GhHome {
    pub fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("subspace-report-test-gh-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("hosts.yml"), "github.com:\n    users:\n        first-test-user: {}\n        active-test-user: {}\n    user: active-test-user\n").unwrap();
        Self(path)
    }

    pub fn environment(&self) -> GhEnvironment {
        GhEnvironment(self.0.to_string_lossy().into_owned())
    }
}

impl Drop for GhHome {
    fn drop(&mut self) {
        std::fs::remove_file(self.0.join("hosts.yml")).unwrap();
        std::fs::remove_dir(&self.0).unwrap();
    }
}

pub(super) struct GhEnvironment(String);

impl Environment for GhEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_CONFIG_DIR").then(|| self.0.clone())
    }
}

pub(super) struct GhCli(pub Arc<Mutex<Vec<Vec<String>>>>);

impl Runner for GhCli {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        remove: &[&str],
        _: Duration,
    ) -> Result<String, String> {
        assert_eq!(program, "gh");
        assert_eq!(remove, ["GH_TOKEN", "GITHUB_TOKEN"]);
        self.0
            .lock()
            .unwrap()
            .push(args.iter().map(|arg| (*arg).into()).collect());
        Ok("test-only-active-account-credential".into())
    }
}
