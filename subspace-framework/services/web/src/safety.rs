use std::net::IpAddr;
use url::Url;

pub(super) fn validate_url(value: &str, allowed_hosts: &[String]) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "web_read requires a valid HTTPS URL")?;
    if url.scheme() != "https" {
        return Err("web_read permits HTTPS URLs only".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("web_read refuses user information in URLs".into());
    }
    let host = parsed_host(value)?;
    if !host_allowed(&host, allowed_hosts) {
        return Err(format!("web_read host is not allowed: {host}"));
    }
    if host.parse::<IpAddr>().is_ok_and(blocked_ip) {
        return Err("web_read refuses a non-public address".into());
    }
    Ok(url)
}

pub(super) fn parsed_host(value: &str) -> Result<String, String> {
    Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .ok_or_else(|| "web_read URL has no host".into())
}

pub(super) fn host_allowed(host: &str, entries: &[String]) -> bool {
    entries.iter().any(
        |entry| match entry.to_ascii_lowercase().strip_prefix("*.") {
            Some(base) => host
                .strip_suffix(&format!(".{base}"))
                .is_some_and(|label| !label.is_empty() && !label.contains('.')),
            None => host.eq_ignore_ascii_case(entry),
        },
    )
}

pub(super) fn blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => blocked_v4(ip.octets(), ip),
        IpAddr::V6(ip) => blocked_v6(ip.segments(), ip),
    }
}

pub(super) fn addresses_are_public(addresses: impl IntoIterator<Item = IpAddr>) -> bool {
    addresses.into_iter().all(|address| !blocked_ip(address))
}

fn blocked_v4(octets: [u8; 4], ip: std::net::Ipv4Addr) -> bool {
    const CGNAT_FIRST: u8 = 100;
    const CGNAT_LOW: u8 = 64;
    const CGNAT_HIGH: u8 = 127;
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_unspecified()
        || octets[0] == 0
        || (octets[0] == CGNAT_FIRST && (CGNAT_LOW..=CGNAT_HIGH).contains(&octets[1]))
}

fn blocked_v6(segments: [u16; 8], ip: std::net::Ipv6Addr) -> bool {
    const UNIQUE_MASK: u16 = 0xfe00;
    const UNIQUE_PREFIX: u16 = 0xfc00;
    const LINK_MASK: u16 = 0xffc0;
    const LINK_PREFIX: u16 = 0xfe80;
    ip.is_loopback()
        || ip.is_multicast()
        || ip.is_unspecified()
        || (segments[0] & UNIQUE_MASK) == UNIQUE_PREFIX
        || (segments[0] & LINK_MASK) == LINK_PREFIX
}

#[derive(Debug)]
pub(super) struct SafeResolver;

impl ureq::unversioned::resolver::Resolver for SafeResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        use ureq::unversioned::resolver::DefaultResolver;
        let addresses = DefaultResolver::default().resolve(uri, config, timeout)?;
        if !addresses_are_public(addresses.iter().map(|address| address.ip())) {
            Err(ureq::Error::HostNotFound)
        } else {
            Ok(addresses)
        }
    }
}
