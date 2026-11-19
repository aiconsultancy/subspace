//! The fixture's offline GitHub client and fictional repository.

use std::{path::PathBuf, sync::Arc, time::Duration};

use subspace_client_github::Client;
use subspace_core::config::Config;
use subspace_core::credentials::{Credentials, Environment, Runner};
use subspace_report_issue::{Destination, IssueRepository};

struct NoEnvironment {
    directory: PathBuf,
}
impl Environment for NoEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_CONFIG_DIR").then(|| self.directory.to_string_lossy().into_owned())
    }
}
struct NoCommands;
impl Runner for NoCommands {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("Fixture has no credentials or external command access".into())
    }
}

/// An offline client, filing into a fictional repository.
pub(crate) fn offline_destination(
    directory: &std::path::Path,
) -> Result<Destination, Box<dyn std::error::Error>> {
    let environment = NoEnvironment {
        directory: directory.into(),
    };
    let credentials = Credentials::new(&Config::default(), environment, NoCommands);
    Ok(Destination {
        client: Arc::new(Client::new(Arc::new(credentials)).with_api_url("http://127.0.0.1:1")),
        repository: IssueRepository::parse("example-org/example-app")
            .ok_or("invalid fixture repository")?,
    })
}
