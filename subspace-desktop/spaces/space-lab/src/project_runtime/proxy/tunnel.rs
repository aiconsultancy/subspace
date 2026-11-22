use anyhow::{Context, Result, ensure};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const IO_POLL: Duration = Duration::from_millis(100);
const HEADER_TIMEOUT: Duration = Duration::from_secs(2);
const HEADER_BYTES: usize = 8192;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const TUNNEL_BUFFER_BYTES: usize = 16 * 1024;

pub(super) fn read_target(client: &mut TcpStream) -> Result<String> {
    client.set_read_timeout(Some(IO_POLL))?;
    client.set_write_timeout(Some(IO_POLL))?;
    let deadline = Instant::now() + HEADER_TIMEOUT;
    let mut bytes = Vec::new();
    while bytes.len() < HEADER_BYTES && Instant::now() < deadline {
        let mut byte = [0u8; 1];
        match client.read(&mut byte) {
            Ok(0) => anyhow::bail!("closed CONNECT request"),
            Ok(_) => bytes.push(byte[0]),
            Err(error) if transient(&error) => continue,
            Err(error) => return Err(error.into()),
        }
        if bytes.ends_with(b"\r\n\r\n") {
            return parse_target(&bytes);
        }
    }
    anyhow::bail!("CONNECT header exceeds time or size budget")
}

fn parse_target(bytes: &[u8]) -> Result<String> {
    let header = std::str::from_utf8(bytes)?;
    let mut parts = header
        .lines()
        .next()
        .context("empty CONNECT request")?
        .split_whitespace();
    ensure!(
        parts.next() == Some("CONNECT"),
        "only HTTPS CONNECT is supported"
    );
    let target = parts.next().context("CONNECT target missing")?;
    ensure!(
        matches!(parts.next(), Some("HTTP/1.0" | "HTTP/1.1")) && parts.next().is_none(),
        "invalid CONNECT request"
    );
    Ok(target.into())
}

pub(super) fn deny(client: &mut TcpStream) {
    let _ = client.write_all(b"HTTP/1.1 403 Destination denied\r\nConnection: close\r\n\r\n");
}

pub(super) fn connect(addresses: &[SocketAddr], cancelled: &AtomicBool) -> Result<TcpStream> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    for address in addresses {
        ensure!(!cancelled.load(Ordering::Acquire), "proxy stopped");
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .context("destination connect timeout")?;
        if let Ok(stream) = TcpStream::connect_timeout(address, remaining.min(IO_POLL)) {
            return Ok(stream);
        }
    }
    anyhow::bail!("destination connection failed")
}

pub(super) fn relay(
    client: &mut TcpStream,
    mut upstream: TcpStream,
    cancelled: &AtomicBool,
) -> Result<()> {
    client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;
    upstream.set_read_timeout(Some(IO_POLL))?;
    upstream.set_write_timeout(Some(IO_POLL))?;
    let mut to_client = client.try_clone()?;
    let mut from_upstream = upstream.try_clone()?;
    std::thread::scope(|scope| {
        let back = scope.spawn(|| copy(&mut from_upstream, &mut to_client, cancelled));
        let result = copy(client, &mut upstream, cancelled);
        let _ = client.shutdown(Shutdown::Both);
        let _ = upstream.shutdown(Shutdown::Both);
        let _ = back.join();
        result
    })
}

fn copy(source: &mut TcpStream, destination: &mut TcpStream, cancelled: &AtomicBool) -> Result<()> {
    let mut bytes = [0u8; TUNNEL_BUFFER_BYTES];
    while !cancelled.load(Ordering::Acquire) {
        match source.read(&mut bytes) {
            Ok(0) => break,
            Ok(count) => destination.write_all(&bytes[..count])?,
            Err(error) if transient(&error) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let _ = destination.shutdown(Shutdown::Both);
    Ok(())
}

fn transient(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
    )
}

#[cfg(test)]
mod tests {
    use super::parse_target;

    #[test]
    fn connect_supports_python_legacy_and_current_http_versions_only() {
        assert_eq!(
            parse_target(b"CONNECT api.example.com:443 HTTP/1.0\r\n\r\n")
                .expect("Python 3.11 CONNECT"),
            "api.example.com:443"
        );
        assert_eq!(
            parse_target(b"CONNECT api.example.com:443 HTTP/1.1\r\n\r\n").expect("current CONNECT"),
            "api.example.com:443"
        );
        assert!(parse_target(b"CONNECT api.example.com:443 HTTP/2\r\n\r\n").is_err());
        assert!(parse_target(b"GET api.example.com:443 HTTP/1.1\r\n\r\n").is_err());
        assert!(parse_target(b"CONNECT api.example.com:443 HTTP/1.1 trailing\r\n\r\n").is_err());
    }
}
