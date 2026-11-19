use anyhow::Result;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::thread;

use super::authority::Authority;

const TRANSFER_LIMIT: u64 = 4 * 1024 * 1024;

pub(super) struct ProxyRequest {
    pub method: String,
    pub authority: Authority,
    pub path: String,
}

pub(super) fn tunnel(mut client: TcpStream, upstream: TcpStream) -> Result<()> {
    client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
    let mut upstream_write = upstream.try_clone()?;
    let client_read = client.try_clone()?;
    let upload = thread::spawn(move || {
        let _ = std::io::copy(&mut client_read.take(TRANSFER_LIMIT), &mut upstream_write);
        let _ = upstream_write.shutdown(Shutdown::Write);
    });
    let _ = std::io::copy(&mut upstream.take(TRANSFER_LIMIT), &mut client);
    let _ = client.shutdown(Shutdown::Both);
    let _ = upload.join();
    Ok(())
}

pub(super) fn forward_http(
    client: &mut TcpStream,
    upstream: &mut TcpStream,
    request: &ProxyRequest,
) -> Result<()> {
    write!(
        upstream,
        "{} {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nUser-Agent: Subspace-Runtime-Lab\r\nConnection: close\r\n\r\n",
        request.method,
        request.path,
        request.authority.text()
    )?;
    std::io::copy(&mut upstream.take(TRANSFER_LIMIT), client)?;
    Ok(())
}
