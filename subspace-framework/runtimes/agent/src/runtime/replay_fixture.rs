//! The four SDK wire calls needed to load a remote replay, with no executable.
use super::{Client, Path, lock};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream};

pub struct ReplayPeer {
    pub client: Client,
    pub requests: Arc<Mutex<Vec<Value>>>,
    /// While set, the peer records requests and answers none, as a hung
    /// runtime does.
    pub hold: Arc<AtomicBool>,
    worker: tokio::task::JoinHandle<()>,
}

impl ReplayPeer {
    pub fn new(workspace: &Path) -> Self {
        let (local, mut remote) = tokio::io::duplex(64 * 1024);
        let (read, write) = tokio::io::split(local);
        let client = Client::from_streams(read, write, workspace.into()).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let hold = Arc::new(AtomicBool::new(false));
        let held = hold.clone();
        let worker = tokio::spawn(async move {
            while let Some(request) = read_request(&mut remote).await {
                lock(&recorded).push(request.clone());
                if held.load(Ordering::SeqCst) {
                    continue;
                }
                let response = response(&request);
                let body = response.to_string();
                let frame = format!("Content-Length: {}\r\n\r\n{body}", body.len());
                remote.write_all(frame.as_bytes()).await.unwrap();
            }
        });
        Self {
            client,
            requests,
            hold,
            worker,
        }
    }
}

impl Drop for ReplayPeer {
    fn drop(&mut self) {
        self.client.force_stop();
        self.worker.abort();
    }
}

async fn read_request(stream: &mut DuplexStream) -> Option<Value> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        header.push(stream.read_u8().await.ok()?);
        assert!(header.len() < 1024, "bounded fixture header");
    }
    let length = std::str::from_utf8(&header)
        .unwrap()
        .trim()
        .strip_prefix("Content-Length: ")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(length < 64 * 1024, "bounded fixture body");
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    Some(serde_json::from_slice(&body).unwrap())
}

fn response(request: &Value) -> Value {
    let result = match request["method"].as_str().unwrap() {
        "sessions.connect" => json!({
            "sessionId":"transient-replay", "metadata": {
                "kind":"coding-agent", "name":"Fixture cloud task", "repository":{"owner":"test","name":"fixture","branch":"main"},
                "sessionId":"transient-replay", "modifiedTime":"2026-11-04T00:00:00Z", "startTime":"2026-11-04T00:00:00Z"
            }
        }),
        "session.resume" | "session.skills.reload" => json!({}),
        "session.eventLog.read" => {
            json!({"cursor":"end","cursorStatus":"ok","events":[],"hasMore":false})
        }
        method => {
            return json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32601,"message":format!("Unexpected fixture call: {method}")}});
        }
    };
    json!({"jsonrpc":"2.0","id":request["id"],"result":result})
}
