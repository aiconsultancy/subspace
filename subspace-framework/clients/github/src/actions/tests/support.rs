use crate::Client;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::Arc,
    thread,
    time::Duration,
};
use subspace_core::{
    config::Config,
    credentials::{Credentials, Environment, Runner},
};

struct Env;
impl Environment for Env {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_TOKEN").then(|| "fixture-token".to_owned())
    }
}
struct NoCli;
impl Runner for NoCli {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("CLI disabled in tests".into())
    }
}

#[derive(Debug)]
pub(super) struct Request {
    pub line: String,
    pub authorization: String,
    pub body: String,
}
pub(super) struct Reply {
    status: u16,
    body: String,
    pub headers: Vec<(String, String)>,
}
impl Reply {
    pub(super) fn json(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            headers: Vec::new(),
        }
    }
    pub(super) fn status(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
            headers: Vec::new(),
        }
    }
}

pub(super) fn serve(handler: impl Fn(&Request) -> Reply + Send + Sync + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handler = Arc::new(handler);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let handler = handler.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let request = read_request(&mut reader);
                let reply = handler(&request);
                let extra = reply
                    .headers
                    .iter()
                    .map(|(name, value)| format!("{name}: {value}\r\n"))
                    .collect::<String>();
                let response = format!(
                    "HTTP/1.1 {} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{}",
                    reply.status,
                    reply.body.len(),
                    reply.body
                );
                stream.write_all(response.as_bytes()).unwrap();
            });
        }
    });
    format!("http://{address}")
}

fn read_request(reader: &mut BufReader<std::net::TcpStream>) -> Request {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let (mut length, mut authorization) = (0, String::new());
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(": ").unwrap_or((header, ""));
        match name.to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse().unwrap(),
            "authorization" => authorization = value.to_owned(),
            _ => {}
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    Request {
        line: line.trim_end().to_owned(),
        authorization,
        body: String::from_utf8(body).unwrap(),
    }
}

pub(super) fn client(base: String) -> Client {
    let credentials = Credentials::new(&Config::default(), Env, NoCli);
    Client::new(Arc::new(credentials)).with_api_url(base)
}
