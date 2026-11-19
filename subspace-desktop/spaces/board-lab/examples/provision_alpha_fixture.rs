//! Explicitly provision an isolated environment for the contained alpha tests.
//! This downloads runtimes/packages. It never starts project code.

use anyhow::{Context, Result, ensure};
use std::path::PathBuf;
use subspace_board_lab_board::environments::EnvironmentManager;

fn main() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let root = PathBuf::from(
        arguments
            .next()
            .context("Expected an absolute isolated environments root")?,
    );
    let id = arguments
        .next()
        .context("Expected python-alt, node-services or bun-notebook")?;
    let id = id.to_str().context("Environment ID must be UTF-8")?;
    ensure!(arguments.next().is_none(), "Unexpected argument");
    ensure!(root.is_absolute(), "Fixture root must be absolute");
    ensure!(
        ["python-alt", "node-services", "bun-notebook"].contains(&id),
        "Unsupported fixture environment"
    );
    let marker = root.join(".alpha-fixture");
    if !root.exists() {
        std::fs::create_dir_all(&root)?;
        std::fs::write(
            &marker,
            "Isolated Board Lab alpha fixture, no user credentials\n",
        )?;
    }
    ensure!(
        marker.is_file(),
        "Refusing to provision a root without the alpha fixture marker"
    );
    let manager = EnvironmentManager::new(root);
    let mut info = manager.provision(id)?;
    if id == "bun-notebook"
        && !info
            .spec
            .packages
            .iter()
            .any(|package| package.starts_with("semver@"))
    {
        info = manager.add_package(id, "semver@7.7.4")?;
    }
    ensure!(info.ready, "Fixture environment is not ready");
    println!("{}: {}", info.spec.id, info.summary);
    Ok(())
}
