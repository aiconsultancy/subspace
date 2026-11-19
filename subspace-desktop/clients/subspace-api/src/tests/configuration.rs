use super::common;
use crate::{Client, Error};
use subspace_framework::client_test_support::Mock;

struct AssertionFile(std::path::PathBuf);

impl AssertionFile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "subspace-assertion-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> std::path::PathBuf {
        self.0.join("assertion")
    }
}

impl Drop for AssertionFile {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
#[cfg(debug_assertions)]
fn debug_assertion_file_is_read_on_creation_and_trailing_newlines_are_trimmed() {
    let file = AssertionFile::new();
    std::fs::write(file.path(), "fixture-first\r\n").unwrap();
    let mock = Mock::new(common);
    let base = format!("{}/api", mock.base_url());
    let first = Client::debug_override(&base, Some(&file.path())).unwrap();
    std::fs::write(file.path(), "fixture-second\n").unwrap();
    let second = Client::debug_override(&base, Some(&file.path())).unwrap();
    std::fs::remove_file(file.path()).unwrap();
    assert!(first.me().is_ok());
    assert!(second.me().is_ok());
    let requests = mock.requests();
    for (request, expected) in requests.iter().zip(["fixture-first", "fixture-second"]) {
        assert!(
            request
                .headers
                .contains(&("authorization".into(), format!("Bearer {expected}")))
        );
    }
    assert!(matches!(
        Client::debug_override(&base, Some(&file.path())),
        Err(Error::InvalidRequest)
    ));
}

#[test]
#[cfg(debug_assertions)]
fn assertion_file_requires_loopback_and_a_single_nonempty_line() {
    let file = AssertionFile::new();
    for text in ["", "\n", "first\nsecond"] {
        std::fs::write(file.path(), text).unwrap();
        assert!(matches!(
            Client::debug_override("http://127.0.0.1/api", Some(&file.path())),
            Err(Error::InvalidRequest)
        ));
    }
    std::fs::write(file.path(), "fixture-only").unwrap();
    for base in [
        "https://api.fixture.example.test/api",
        "http://localhost/api",
        "http://127.0.0.1.example.invalid/api",
    ] {
        assert!(matches!(
            Client::debug_override(base, Some(&file.path())),
            Err(Error::InvalidRequest)
        ));
    }
}

#[test]
#[cfg(debug_assertions)]
fn loopback_agent_disables_proxies_and_production_keeps_the_shared_configuration() {
    for base in ["http://127.0.0.1/api", "http://[::1]/api"] {
        let client = Client::loopback(base, None).unwrap();
        assert!(client.agent.config().proxy().is_none());
    }
    let production = crate::transport::agent_for(
        &url::Url::parse("https://api.fixture.example.test/api").unwrap(),
    );
    let shared = subspace_framework::core::http::agent();
    assert_eq!(
        format!("{:?}", production.config().proxy()),
        format!("{:?}", shared.config().proxy())
    );
}
