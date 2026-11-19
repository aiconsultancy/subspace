//! The network variables a child process such as the Azure CLI gets: proxy
//! settings and certificate bundles.

/// Proxy variables, upper and lower case, as curl, Python and Go read them.
pub const PROXY_VARIABLES: [&str; 8] = [
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// Certificate bundle variables that Python `requests` and OpenSSL read.
pub const CERTIFICATE_VARIABLES: [&str; 3] =
    ["REQUESTS_CA_BUNDLE", "SSL_CERT_FILE", "SSL_CERT_DIR"];

/// Whether `name` is one of the network variables, exactly as written.
pub fn is_network_variable(name: &str) -> bool {
    PROXY_VARIABLES.contains(&name) || CERTIFICATE_VARIABLES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::is_network_variable;

    #[test]
    fn network_variables_match_exactly_as_written() {
        assert!(is_network_variable("HTTPS_PROXY"));
        assert!(is_network_variable("no_proxy"));
        assert!(is_network_variable("SSL_CERT_FILE"));
        assert!(!is_network_variable("Https_Proxy"));
        assert!(!is_network_variable("PATH"));
    }
}
