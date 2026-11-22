use anyhow::{Context, Result, ensure};
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use url::Url;

use super::authority::Authority;
use super::connect::connect;
use super::io::{read_head, response};
use super::tunnel::{ProxyRequest, forward_http, tunnel};

const HTTP_LINE_PARTS: usize = 3;

pub(super) fn serve(
    mut client: TcpStream,
    rules: &[Authority],
    local_fixture: bool,
    allowed: &AtomicUsize,
    denied: &AtomicUsize,
) -> Result<()> {
    let request = parse_request(&read_head(&mut client)?)?;
    if !rules.contains(&request.authority) || !method_allowed(&request.method) {
        denied.fetch_add(1, Ordering::Relaxed);
        return response(
            &mut client,
            "403 Forbidden",
            r#"{"error":"destination or method is not allowed"}"#,
        );
    }
    relay(client, request, local_fixture, allowed, denied)
}

fn method_allowed(method: &str) -> bool {
    method == "CONNECT" || method == "GET" || method == "HEAD"
}

fn parse_request(head: &str) -> Result<ProxyRequest> {
    let parts = head
        .lines()
        .next()
        .context("No HTTP request")?
        .split_whitespace()
        .collect::<Vec<_>>();
    ensure!(parts.len() == HTTP_LINE_PARTS, "Malformed proxy request");
    let (method, target) = (parts[0], parts[1]);
    if method == "CONNECT" {
        return Ok(ProxyRequest {
            method: method.to_owned(),
            authority: Authority::parse(target)?,
            path: String::new(),
        });
    }
    absolute_http(method, target)
}

fn absolute_http(method: &str, target: &str) -> Result<ProxyRequest> {
    let url = Url::parse(target)?;
    ensure!(
        url.scheme() == "http" && url.username().is_empty() && url.password().is_none(),
        "Only absolute HTTP URLs or HTTPS CONNECT are supported"
    );
    Ok(ProxyRequest {
        method: method.to_owned(),
        authority: Authority {
            host: url
                .host_str()
                .context("Missing proxy host")?
                .to_ascii_lowercase(),
            port: url.port_or_known_default().context("Missing proxy port")?,
        },
        path: request_path(&url),
    })
}

fn request_path(url: &Url) -> String {
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    path
}

fn relay(
    mut client: TcpStream,
    request: ProxyRequest,
    local_fixture: bool,
    allowed: &AtomicUsize,
    denied: &AtomicUsize,
) -> Result<()> {
    let mut upstream = match connect(&request.authority, local_fixture) {
        Ok(stream) => stream,
        Err(_) => {
            denied.fetch_add(1, Ordering::Relaxed);
            return response(
                &mut client,
                "502 Bad Gateway",
                r#"{"error":"approved upstream unavailable or address policy rejected it"}"#,
            );
        }
    };
    allowed.fetch_add(1, Ordering::Relaxed);
    if request.method == "CONNECT" {
        tunnel(client, upstream)
    } else {
        forward_http(&mut client, &mut upstream, &request)
    }
}
