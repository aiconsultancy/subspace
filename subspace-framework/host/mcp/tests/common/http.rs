//! A raw HTTP/1.1 client for the endpoint: each body frame is kept as it
//! arrives, so a test can see that a response streamed.

use http_body_util::BodyExt;
use serde_json::Value;
use subspace_mcp::{Endpoint, Options};

use super::common::{MODERN, server};

pub struct Http {
    pub address: std::net::SocketAddr,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Http {
    pub fn at(address: std::net::SocketAddr) -> Self {
        Self {
            address,
            stop: None,
        }
    }

    pub async fn start(arguments: &[&str], token: Option<&str>) -> Self {
        let mut words: Vec<String> = vec!["--http".into(), "--bind".into(), "127.0.0.1:0".into()];
        words.extend(arguments.iter().map(|word| (*word).to_owned()));
        let options = parse(&words, token);
        let endpoint = Endpoint::bind(server(), options.http.as_ref().unwrap())
            .await
            .unwrap();
        let address = endpoint.address().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        tokio::spawn(endpoint.serve(async {
            let _ = stopped.await;
        }));
        Self {
            address,
            stop: Some(stop),
        }
    }
}

impl Drop for Http {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

fn parse(words: &[String], token: Option<&str>) -> Options {
    subspace_mcp::parse_options("subspace", words, token.map(str::to_owned)).unwrap()
}

/// The status, the headers, and each body frame as it arrived.
pub struct Reply {
    pub status: u16,
    pub headers: http::HeaderMap,
    pub frames: Vec<String>,
}

impl Reply {
    pub fn body(&self) -> String {
        self.frames.concat()
    }

    /// The JSON of each SSE `data:` line, in order.
    pub fn events(&self) -> Vec<Value> {
        self.body()
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .filter_map(|data| serde_json::from_str(data.trim()).ok())
            .collect()
    }
}

pub async fn send(
    server: &Http,
    method: http::Method,
    headers: &[(&str, String)],
    body: Option<Value>,
) -> Reply {
    let stream = tokio::net::TcpStream::connect(server.address)
        .await
        .unwrap();
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
            .await
            .unwrap();
    tokio::spawn(connection);
    let mut builder = http::Request::builder()
        .method(method)
        .uri("/mcp")
        .header("host", server.address.to_string());
    for (name, value) in headers {
        builder = builder.header(*name, value);
    }
    let body = body.map(|body| body.to_string()).unwrap_or_default();
    let request = builder
        .body(http_body_util::Full::new(bytes::Bytes::from(body)))
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    let (parts, mut body) = response.into_parts();
    let mut frames = Vec::new();
    while let Some(frame) = body.frame().await {
        if let Ok(data) = frame.unwrap().into_data() {
            frames.push(String::from_utf8_lossy(&data).into_owned());
        }
    }
    Reply {
        status: parts.status.as_u16(),
        headers: parts.headers,
        frames,
    }
}

pub fn modern_headers(method: &str, name: Option<&str>) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("content-type", "application/json".to_owned()),
        ("accept", "application/json, text/event-stream".to_owned()),
        ("mcp-protocol-version", MODERN.to_owned()),
        ("mcp-method", method.to_owned()),
    ];
    if let Some(name) = name {
        headers.push(("mcp-name", name.to_owned()));
    }
    headers
}
