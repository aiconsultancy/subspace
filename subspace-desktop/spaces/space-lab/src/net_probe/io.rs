use anyhow::{Result, ensure};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const HEADER_LIMIT: usize = 16 * 1024;
const STREAM_IO_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn timeouts(stream: &TcpStream) -> Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(STREAM_IO_TIMEOUT))?;
    stream.set_write_timeout(Some(STREAM_IO_TIMEOUT))?;
    Ok(())
}

pub(super) fn read_head(stream: &mut TcpStream) -> Result<String> {
    timeouts(stream)?;
    let mut bytes = Vec::new();
    let mut byte = [0];
    while bytes.len() < HEADER_LIMIT {
        ensure!(
            stream.read(&mut byte)? == 1,
            "Connection closed before HTTP headers"
        );
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return Ok(String::from_utf8(bytes)?);
        }
    }
    anyhow::bail!("HTTP headers exceed 16 KiB")
}

pub(super) fn response(stream: &mut TcpStream, status: &str, body: &str) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    Ok(())
}
