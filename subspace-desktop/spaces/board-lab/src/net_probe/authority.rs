use anyhow::{Context, Result, ensure};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::Url;

const HOST_MAX_LEN: usize = 253;
const CLASS_D_START: u8 = 224;
const CGNAT_PREFIX: u8 = 100;
const CGNAT_SECOND_MIN: u8 = 64;
const CGNAT_SECOND_MAX: u8 = 127;
pub(super) const PUBLIC_HTTP_PORT: u16 = 80;
pub(super) const PUBLIC_HTTPS_PORT: u16 = 443;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Authority {
    pub host: String,
    pub port: u16,
}

impl Authority {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        ensure!(
            value.len() <= HOST_MAX_LEN && !value.contains(['/', '\\', '@', '\n', '\r']),
            "Use an exact host:port, not a URL or credentials"
        );
        let url = Url::parse(&format!("http://{value}"))?;
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Invalid proxy authority"
        );
        Ok(Self {
            host: url.host_str().context("Missing host")?.to_ascii_lowercase(),
            port: url.port_or_known_default().context("Missing port")?,
        })
    }

    pub(super) fn text(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

pub(super) fn public_port(port: u16) -> bool {
    port == PUBLIC_HTTP_PORT || port == PUBLIC_HTTPS_PORT
}

pub(crate) fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_ipv4(ip),
        IpAddr::V6(ip) => public_ipv6(ip),
    }
}

fn public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !ip.is_private()
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_unspecified()
        && !ip.is_multicast()
        && octets[0] != 0
        && octets[0] < CLASS_D_START
        && !is_cgnat(octets)
}

fn public_ipv6(ip: Ipv6Addr) -> bool {
    ip.to_ipv4_mapped()
        .map(public_ipv4)
        .unwrap_or_else(|| native_public_ipv6(ip))
}

fn native_public_ipv6(ip: Ipv6Addr) -> bool {
    !ip.is_loopback()
        && !ip.is_unspecified()
        && !ip.is_unique_local()
        && !ip.is_unicast_link_local()
        && !ip.is_multicast()
}

fn is_cgnat(octets: [u8; 4]) -> bool {
    octets[0] == CGNAT_PREFIX && (CGNAT_SECOND_MIN..=CGNAT_SECOND_MAX).contains(&octets[1])
}
