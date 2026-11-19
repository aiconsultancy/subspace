//! Installed external editors for ordinary local dashboard projects.

use std::path::{Path, PathBuf};

#[cfg(unix)]
const EXECUTABLE_PERMISSION_BITS: u32 = 0o111;

/// A detected editor. Discovery never installs or starts an application.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InstalledEditor {
    pub id: String,
    pub title: String,
    pub program: PathBuf,
    pub prefix: Vec<String>,
}

/// Detect supported editors using the shared tool lookup and platform
/// application locations.
pub fn discover() -> Vec<InstalledEditor> {
    discover_at(
        &|name| subspace_framework::core::path::find_tool(name).ok(),
        &application_roots(),
        std::env::consts::OS,
    )
}

/// `find` returns the program a command name runs, if any.
type FindTool<'a> = &'a dyn Fn(&str) -> Option<PathBuf>;

fn discover_at(find: FindTool<'_>, roots: &[PathBuf], platform: &str) -> Vec<InstalledEditor> {
    [("vscode", "VS Code"), ("zed", "Zed")]
        .into_iter()
        .filter_map(|(id, title)| {
            cli_editor(find, platform, id, title)
                .or_else(|| application_editor(roots, platform, id, title))
        })
        .collect()
}

fn cli_editor(
    find: FindTool<'_>,
    platform: &str,
    id: &str,
    title: &str,
) -> Option<InstalledEditor> {
    let names: &[&str] = match (platform, id) {
        ("windows", "vscode") => &["Code.exe", "code.exe"],
        ("windows", _) => &["zed.exe"],
        (_, "vscode") => &["code"],
        _ => &["zed"],
    };
    names
        .iter()
        .find_map(|name| find(name))
        .map(|program| InstalledEditor {
            id: id.into(),
            title: title.into(),
            program,
            prefix: Vec::new(),
        })
}

fn application_editor(
    roots: &[PathBuf],
    platform: &str,
    id: &str,
    title: &str,
) -> Option<InstalledEditor> {
    let relative = match (platform, id) {
        ("macos", "vscode") => "Visual Studio Code.app",
        ("macos", _) => "Zed.app",
        ("windows", "vscode") => "Microsoft VS Code/Code.exe",
        ("windows", _) => "Zed/zed.exe",
        _ => return None,
    };
    let app = roots.iter().map(|root| root.join(relative)).find(|path| {
        if platform == "macos" {
            path.is_dir()
        } else {
            executable(path)
        }
    })?;
    let (program, prefix) = if platform == "macos" {
        (
            PathBuf::from("/usr/bin/open"),
            vec!["-a".into(), app.to_str()?.into()],
        )
    } else {
        (app, Vec::new())
    };
    Some(InstalledEditor {
        id: id.into(),
        title: title.into(),
        program,
        prefix,
    })
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        metadata.permissions().mode() & EXECUTABLE_PERMISSION_BITS != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn application_roots() -> Vec<PathBuf> {
    match std::env::consts::OS {
        "macos" => {
            let mut roots = vec![PathBuf::from("/Applications")];
            roots.extend(
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications")),
            );
            roots
        }
        "windows" => ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(|name| std::env::var_os(name).map(|value| (name, PathBuf::from(value))))
            .map(|(name, path)| {
                if name == "LOCALAPPDATA" {
                    path.join("Programs")
                } else {
                    path
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

impl InstalledEditor {
    /// Resolve a literal launch plan. No shell parses the project path.
    pub fn command_for(&self, project: &Path) -> std::io::Result<std::process::Command> {
        let canonical = project.canonicalize()?;
        if !canonical.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "The dashboard project is not a directory",
            ));
        }
        let mut command = std::process::Command::new(&self.program);
        command.args(&self.prefix).arg(canonical);
        Ok(command)
    }

    /// Open a project only after an explicit user action, off the UI thread.
    pub fn open_project(&self, project: &Path) -> std::io::Result<()> {
        let status = self.command_for(project)?.status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(
                "The selected editor could not open the project",
            ))
        }
    }
}

#[cfg(test)]
#[path = "editors/tests.rs"]
mod tests;
