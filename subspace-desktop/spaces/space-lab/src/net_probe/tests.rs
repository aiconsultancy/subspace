use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::authority::{Authority, public_ip};
use super::{configure_at, policies_at};
use crate::environments::EnvironmentManager;

#[test]
fn public_ip_allows_global_unicast_and_rejects_private_ranges() {
    assert!(public_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    assert!(public_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
    assert!(!public_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
    assert!(!public_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    assert!(!public_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
    assert!(!public_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    assert!(!public_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
    assert!(!public_ip(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))));
    assert!(!public_ip(IpAddr::V4(Ipv4Addr::new(224, 0, 0, 1))));
    assert!(!public_ip(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
}

#[test]
fn public_ip_applies_ipv4_mapped_ipv6_to_the_same_policy() {
    assert!(public_ip("::ffff:8.8.8.8".parse().unwrap()));
    assert!(!public_ip("::ffff:10.0.0.1".parse().unwrap()));
    assert!(!public_ip("::ffff:192.168.0.1".parse().unwrap()));
    assert!(!public_ip("::ffff:127.0.0.1".parse().unwrap()));
    assert!(!public_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    assert!(!public_ip("fc00::1".parse().unwrap()));
    assert!(!public_ip("fe80::1".parse().unwrap()));
    assert!(!public_ip(IpAddr::V6(Ipv6Addr::UNSPECIFIED)));
}

#[test]
fn authority_parse_accepts_host_port_and_rejects_urls_or_credentials() {
    let github = Authority::parse("api.github.com:443").expect("public host");
    assert_eq!(github.host, "api.github.com");
    assert_eq!(github.port, 443);
    assert!(Authority::parse("http://api.github.com:443").is_err());
    assert!(Authority::parse("user:pass@api.github.com:443").is_err());
    assert!(Authority::parse("api.github.com/path").is_err());
    assert!(Authority::parse("api.github.com:443\n").is_err());
}

#[test]
fn default_policies_are_pure_and_do_not_create_files() {
    let directory = tempfile::tempdir().expect("temporary test directory");
    let root = directory.path().join("space-lab");
    let policies = policies_at(&root).expect("defaults");
    assert_eq!(
        policies.get("python-data").map(Vec::as_slice),
        Some(["api.github.com:443".to_string()].as_slice())
    );
    assert!(policies.get("cli-lab").is_some_and(Vec::is_empty));
    assert!(!root.exists());
}

#[test]
fn configure_writes_only_under_the_injected_root() {
    let directory = tempfile::tempdir().expect("temporary test directory");
    let root = directory.path().join("space-lab");
    let manager = EnvironmentManager::new(root.join("environments"));
    let output = configure_at(
        &root,
        &manager,
        "python-data",
        vec!["example.com:443".into()],
    )
    .expect("configure");
    let file = root.join("network-policies.json");
    assert!(file.is_file());
    assert!(file.starts_with(&root));
    assert_eq!(
        output["network_policies"]["python-data"][0],
        "example.com:443"
    );
}

#[test]
fn configure_rejects_non_http_ports_without_writing() {
    let directory = tempfile::tempdir().expect("temporary test directory");
    let root = directory.path().join("space-lab");
    let manager = EnvironmentManager::new(root.join("environments"));
    let error = configure_at(
        &root,
        &manager,
        "python-data",
        vec!["example.com:22".into()],
    )
    .unwrap_err();
    assert!(error.to_string().contains("ports 80 and 443"));
    assert!(!root.join("network-policies.json").exists());
}
