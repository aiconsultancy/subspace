use std::path::{Path, PathBuf};

use mxc_sdk::v1::{
    ContainerRequest, Containment, FilesystemPolicy, NetworkAction, NetworkEgressPolicy,
    NetworkIngressPolicy, NetworkPolicy, NetworkRuntimeConfig, configs::SeatbeltConfig,
};

use crate::env;
use crate::selected_environment;

const TIMEOUT_MS: u32 = 15_000;

pub(crate) fn request(
    command: String,
    readonly_roots: &[PathBuf],
    workspace: &Path,
    host_home: Option<&Path>,
    proxy: Option<&str>,
) -> ContainerRequest {
    container_request(
        command,
        filesystem_policy(readonly_roots, workspace, host_home),
        network_policy(proxy),
        env::scoped_environment(readonly_roots, workspace),
        workspace,
    )
}

pub(super) fn persistent_request(
    mut request: ContainerRequest,
    selected: Vec<(String, String)>,
) -> ContainerRequest {
    request.environment = Some(selected_environment::persistent_environment(
        request.environment.take().unwrap_or_default(),
        selected,
    ));
    request.timeout_ms = Some(0);
    request
}

fn filesystem_policy(
    readonly_roots: &[PathBuf],
    workspace: &Path,
    host_home: Option<&Path>,
) -> FilesystemPolicy {
    make_filesystem(
        workspace.to_string_lossy().into_owned(),
        env::path_strings(readonly_roots),
        env::denied_secret_paths(host_home, readonly_roots, workspace),
    )
}

fn make_filesystem(
    readwrite: String,
    readonly: Vec<String>,
    denied: Vec<String>,
) -> FilesystemPolicy {
    FilesystemPolicy {
        readwrite_paths: vec![readwrite],
        readonly_paths: readonly,
        denied_paths: denied,
        ..Default::default()
    }
}

fn network_policy(proxy: Option<&str>) -> NetworkPolicy {
    NetworkPolicy {
        egress: Some(NetworkEgressPolicy {
            default: Some(NetworkAction::Deny),
            ..Default::default()
        }),
        ingress: Some(NetworkIngressPolicy {
            default: Some(NetworkAction::Deny),
            host_loopback: Some(NetworkAction::Deny),
        }),
        runtime_config: proxy.map(|address| NetworkRuntimeConfig {
            network_proxy: Some(address.into()),
        }),
    }
}

fn container_request(
    command: String,
    filesystem: FilesystemPolicy,
    network: NetworkPolicy,
    environment: Vec<(String, String)>,
    workspace: &Path,
) -> ContainerRequest {
    ContainerRequest {
        command: command.clone(),
        filesystem: Some(filesystem),
        network: Some(network),
        containment: Containment::Seatbelt(SeatbeltConfig::default()),
        working_directory: Some(workspace.to_string_lossy().into_owned()),
        environment: Some(environment),
        inherit_default_environment: Some(false),
        timeout_ms: Some(TIMEOUT_MS),
        ..ContainerRequest::new(command)
    }
}

#[cfg(test)]
mod tests {
    use super::{persistent_request, request};
    use mxc_sdk::v1::{Containment, NetworkAction};
    use std::path::{Path, PathBuf};

    #[test]
    fn persistent_policy_only_removes_timeout_and_adds_explicit_environment() {
        let roots = vec![PathBuf::from("/runtime")];
        let workspace = Path::new("/workspace");
        let one_shot = request("'/bin/cat'".into(), &roots, workspace, None, None);
        let persistent = persistent_request(
            one_shot,
            vec![("SELECTED_TEST_VALUE".into(), "dummy-test-only-value".into())],
        );
        assert_eq!(persistent.timeout_ms, Some(0));
        assert_eq!(persistent.inherit_default_environment, Some(false));
        assert!(matches!(persistent.containment, Containment::Seatbelt(_)));
        assert!(
            persistent
                .environment
                .as_ref()
                .unwrap()
                .contains(&("SELECTED_TEST_VALUE".into(), "dummy-test-only-value".into(),))
        );
        assert!(!persistent.command.contains("dummy-test-only-value"));
        let filesystem = persistent.filesystem.unwrap();
        assert_eq!(filesystem.readonly_paths, vec!["/runtime"]);
        assert_eq!(filesystem.readwrite_paths, vec!["/workspace"]);
        let network = persistent.network.unwrap();
        assert_eq!(network.egress.unwrap().default, Some(NetworkAction::Deny));
        let ingress = network.ingress.unwrap();
        assert_eq!(ingress.default, Some(NetworkAction::Deny));
        assert_eq!(ingress.host_loopback, Some(NetworkAction::Deny));
    }

    #[test]
    fn one_shot_timeout_and_proxy_policy_are_preserved() {
        let request = request(
            "'/bin/cat'".into(),
            &[PathBuf::from("/runtime")],
            Path::new("/workspace"),
            None,
            Some("127.0.0.1:12345"),
        );
        assert_eq!(request.timeout_ms, Some(15_000));
        let network = request.network.unwrap();
        assert_eq!(network.egress.unwrap().default, Some(NetworkAction::Deny));
        assert_eq!(
            network.runtime_config.unwrap().network_proxy.as_deref(),
            Some("127.0.0.1:12345")
        );
    }
}
