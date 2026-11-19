//! Reading the `env` output a login shell prints between the markers.

use super::network::is_network_variable;
use super::{END, START};

/// The `PATH=` value between the markers in `env` output from a shell whose
/// startup files may print their own text.
pub fn parse_shell_output(output: &str) -> Option<String> {
    let start = output.find(START)? + START.len();
    let end = start + output[start..].find(END)?;
    output[start..end]
        .lines()
        .find_map(|line| line.strip_prefix("PATH="))
        .map(|value| value.trim_end_matches('\r').to_string())
        .filter(|value| !value.is_empty())
}

/// The network variables ([`is_network_variable`]) between the markers in
/// `env` output, in the order the shell printed them.
pub fn parse_network_variables(output: &str) -> Vec<(String, String)> {
    let Some(start) = output.find(START).map(|index| index + START.len()) else {
        return Vec::new();
    };
    let end = output[start..]
        .find(END)
        .map_or(output.len(), |end| start + end);
    output[start..end]
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, value)| is_network_variable(name) && !value.is_empty())
        .map(|(name, value)| (name.to_string(), value.trim_end_matches('\r').to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::{END, START};
    use super::{parse_network_variables, parse_shell_output};

    #[test]
    fn shell_output_with_startup_noise_yields_path() {
        let output = format!(
            "Welcome!\n{START}\nHOME=/fixture/home\nPATH=/opt/homebrew/bin:/usr/bin\nX=PATH=no\n{END}\nbye\n"
        );
        assert_eq!(
            parse_shell_output(&output).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin")
        );
        assert_eq!(parse_shell_output("PATH=/usr/bin\n"), None);
        assert_eq!(parse_shell_output(&format!("{START}\nPATH=\n{END}")), None);
    }

    #[test]
    fn only_network_variables_between_the_markers_are_read() {
        let output = format!(
            "motd\n{START}\nPATH=/usr/bin\nHTTPS_PROXY=http://proxy.fixture.invalid:8080\nno_proxy=localhost\nREQUESTS_CA_BUNDLE=/etc/fixture/ca.pem\nHTTP_PROXY=\nHOME=/fixture\n{END}\nHTTPS_PROXY=after-the-marker\n"
        );
        assert_eq!(
            parse_network_variables(&output),
            [
                (
                    "HTTPS_PROXY".to_string(),
                    "http://proxy.fixture.invalid:8080".to_string()
                ),
                ("no_proxy".to_string(), "localhost".to_string()),
                (
                    "REQUESTS_CA_BUNDLE".to_string(),
                    "/etc/fixture/ca.pem".to_string()
                ),
            ]
        );
        assert!(parse_network_variables("no markers\nHTTPS_PROXY=x\n").is_empty());
    }
}
