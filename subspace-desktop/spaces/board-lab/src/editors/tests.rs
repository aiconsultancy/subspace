use super::{InstalledEditor, discover_at, executable};
use std::fs;
use std::path::{Path, PathBuf};

/// A tool lookup over one test folder, as the shared lookup does on PATH.
fn find_in(directory: &Path) -> impl Fn(&str) -> Option<PathBuf> + '_ {
    move |name| Some(directory.join(name)).filter(|path| executable(path))
}

fn find_nothing(_: &str) -> Option<PathBuf> {
    None
}
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "board-lab-editor-test-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("test directory");
        Self(path)
    }

    fn executable(&self, name: &str) {
        let path = self.0.join(name);
        fs::write(&path, "test-only editor placeholder").expect("placeholder");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("test mode");
        }
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn path_discovery_finds_both_editors_without_starting_them() {
    let directory = Directory::new();
    directory.executable("code");
    directory.executable("zed");
    let editors = discover_at(&find_in(&directory.0), &[], "linux");
    assert_eq!(
        editors
            .iter()
            .map(|editor| editor.id.as_str())
            .collect::<Vec<_>>(),
        ["vscode", "zed"]
    );
}

#[test]
fn macos_bundle_fallback_is_literal_and_cli_takes_priority() {
    let directory = Directory::new();
    fs::create_dir(directory.0.join("Visual Studio Code.app")).expect("test bundle");
    fs::create_dir(directory.0.join("Zed.app")).expect("test bundle");
    let roots = std::slice::from_ref(&directory.0);
    let bundle = discover_at(&find_nothing, roots, "macos");
    assert_eq!(bundle.len(), 2);
    assert_eq!(bundle[0].program, PathBuf::from("/usr/bin/open"));
    assert_eq!(
        bundle[0].prefix,
        [
            "-a",
            directory
                .0
                .join("Visual Studio Code.app")
                .to_str()
                .expect("test path")
        ]
    );
    directory.executable("code");
    let preferred = discover_at(&find_in(&directory.0), roots, "macos");
    assert_eq!(preferred[0].program, directory.0.join("code"));
    assert!(preferred[0].prefix.is_empty());
}

#[test]
fn command_keeps_shell_metacharacters_in_one_path_argument() {
    let directory = Directory::new();
    let project = directory.0.join("project with spaces;not-a-command");
    fs::create_dir(&project).expect("test project");
    let editor = InstalledEditor {
        id: "zed".into(),
        title: "Zed".into(),
        program: "/test/editor".into(),
        prefix: Vec::new(),
    };
    let command = editor.command_for(&project).expect("launch plan");
    assert_eq!(command.get_program(), "/test/editor");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [project
            .canonicalize()
            .expect("canonical project")
            .as_os_str()]
    );
    assert!(editor.command_for(&directory.0.join("missing")).is_err());
    assert!(editor.open_project(&project).is_err());
}

#[test]
fn missing_or_nonexecutable_editors_are_not_substituted() {
    let directory = Directory::new();
    fs::create_dir(directory.0.join("code")).expect("not an editor executable");
    fs::write(directory.0.join("zed"), "not executable").expect("placeholder");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(directory.0.join("zed"), fs::Permissions::from_mode(0o600))
            .expect("test mode");
        assert!(discover_at(&find_in(&directory.0), &[], "linux").is_empty());
    }
    assert!(discover_at(&find_nothing, &[], "linux").is_empty());
}
