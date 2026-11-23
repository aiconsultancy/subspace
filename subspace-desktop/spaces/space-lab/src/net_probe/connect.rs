use anyhow::{Result, ensure};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::authority::{Authority, public_ip};
use super::io::timeouts;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);

pub(super) fn connect(authority: &Authority, local_fixture: bool) -> Result<TcpStream> {
    let addresses = (authority.host.as_str(), authority.port)
        .to_socket_addrs()?
        .collect::<Vec<_>>();
    ensure!(!addresses.is_empty(), "Destination has no addresses");
    ensure!(
        local_fixture || addresses.iter().all(|address| public_ip(address.ip())),
        "Private, loopback and link-local destinations are not supported by this public-network prototype"
    );
    connect_first(&addresses)
}

fn connect_first(addresses: &[std::net::SocketAddr]) -> Result<TcpStream> {
    let mut last = None;
    for address in addresses {
        match TcpStream::connect_timeout(address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                timeouts(&stream)?;
                return Ok(stream);
            }
            Err(error) => last = Some(error),
        }
    }
    anyhow::bail!("Destination connection failed: {:?}", last)
}
