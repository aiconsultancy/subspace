// Stamps the build the app reports through `ProductBuild`, sets its build
// configuration, including the optional organisation branding, and
// embeds the Windows resources.

#[path = "build/branding.rs"]
mod branding;
#[path = "build/config.rs"]
mod config;
// The run-time override checks are the app's; the build script uses the rest.
#[allow(dead_code)]
#[path = "build/settings.rs"]
mod settings;
#[path = "build/stamp.rs"]
mod stamp;
#[path = "build/windows_resources.rs"]
mod windows_resources;

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    windows_resources::embed().map_err(|error: embed_resource::CompilationResult| {
        format!("Could not embed Windows resources: {error:?}")
    })?;
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let output = PathBuf::from(std::env::var("OUT_DIR")?);
    let config = std::env::var_os(branding::CONFIG_ENV)
        .map_or_else(|| branding::DEFAULT_CONFIG.into(), PathBuf::from);
    branding::embed(&manifest.join(config), &output)?;
    for line in stamp::directives(&build_env())
        .into_iter()
        .chain(config::directives(&overrides()))
    {
        println!("{line}");
    }
    Ok(())
}

fn overrides() -> config::Overrides {
    config::Overrides {
        github_oauth_client_id: std::env::var(config::GITHUB_OAUTH_CLIENT_ID_ENV).ok(),
        report_issue_repository: std::env::var(config::REPORT_ISSUE_REPOSITORY_ENV).ok(),
    }
}

// qual:allow(test_quality, untested) reason: "Reads Cargo's build-script environment, which exists only while the build script runs; the stamp it feeds is tested in the app's tests."
fn build_env() -> stamp::BuildEnv {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    stamp::BuildEnv {
        // products/<product>/ inside this workspace.
        workspace: manifest.join("../.."),
        source_commit: std::env::var("SUBSPACE_SOURCE_COMMIT").ok(),
        target: std::env::var("TARGET").unwrap_or_else(|_| "unknown".into()),
        profile: std::env::var("PROFILE").unwrap_or_else(|_| "unknown".into()),
        capture: std::env::var_os("CARGO_FEATURE_CAPTURE").is_some(),
        machine: stamp::machine_name(),
        unix_time: stamp::timestamp::now(),
    }
}
