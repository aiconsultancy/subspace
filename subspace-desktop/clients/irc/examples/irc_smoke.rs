//! Connects to one network and prints every event.
//!
//! `cargo run -p subspace-client-irc --example irc_smoke -- <host> <port> <nick> [#channel] [--tls] [--insecure]`
//!
//! `--insecure` accepts any TLS certificate, for local test servers.
//!
//! `cargo run -p subspace-client-irc --example irc_smoke -- --config <network.json>`
//! connects with one network entry as written in `networks` of the IRC settings file
//! (`irc/config.json` beside Subspace's config file).

use std::time::Duration;

use subspace_client_irc::{Command, NetworkConfig, NetworkId, Runtime};

const POLL: Duration = Duration::from_millis(100);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match args.iter().position(|arg| arg == "--config") {
        Some(index) => from_file(args.get(index + 1)),
        None => from_args(&args),
    };
    run(config);
}

fn from_file(path: Option<&String>) -> NetworkConfig {
    let text = path.and_then(|path| std::fs::read_to_string(path).ok());
    match text.as_deref().map(serde_json::from_str) {
        Some(Ok(config)) => config,
        _ => {
            eprintln!("--config needs a readable network JSON file");
            std::process::exit(2);
        }
    }
}

fn from_args(args: &[String]) -> NetworkConfig {
    let tls = args.iter().any(|arg| arg == "--tls");
    let accept_invalid_certs = args.iter().any(|arg| arg == "--insecure");
    let mut positional = args.iter().filter(|arg| !arg.starts_with("--"));
    let (Some(host), Some(port), Some(nick)) =
        (positional.next(), positional.next(), positional.next())
    else {
        eprintln!("usage: irc_smoke <host> <port> <nick> [#channel] [--tls] [--insecure]");
        std::process::exit(2);
    };
    let Ok(port) = port.parse() else {
        eprintln!("invalid port: {port}");
        std::process::exit(2);
    };
    NetworkConfig {
        id: NetworkId("smoke".into()),
        name: host.to_string(),
        host: host.to_string(),
        port,
        tls,
        accept_invalid_certs,
        nickname: nick.to_string(),
        alt_nicks: Vec::new(),
        username: String::new(),
        realname: String::new(),
        password: String::new(),
        sasl: None,
        channels: positional.next().cloned().into_iter().collect(),
        auto_connect: true,
        highlights: Vec::new(),
        ca_cert: None,
        client_cert: None,
        client_key: None,
    }
}

fn run(config: NetworkConfig) {
    let runtime = Runtime::start();
    runtime.send(Command::Configure(vec![config]));
    loop {
        for event in runtime.drain() {
            println!("{event:?}");
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::from_args;

    #[test]
    fn arguments_name_the_server_nick_channel_and_tls() {
        let args = [
            "irc.example.test",
            "6697",
            "fixture",
            "#subspace",
            "--tls",
            "--insecure",
        ]
        .map(String::from);
        let config = from_args(&args);
        assert_eq!(config.host, "irc.example.test");
        assert_eq!(config.port, 6697);
        assert_eq!(config.nickname, "fixture");
        assert_eq!(config.channels, ["#subspace"]);
        assert!(config.tls && config.accept_invalid_certs && config.auto_connect);
        let plain = from_args(&["127.0.0.1", "6667", "fixture"].map(String::from));
        assert!(!plain.tls && plain.channels.is_empty());
    }
}
