//! macOS application-bundle install (REQ-05): expand the release zip beside
//! the installed bundle, check its identity and code signature, then swap the
//! two bundles with a rollback. CLI links into the bundle's executable keep
//! working because the bundle path does not change.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};

use crate::UpdateProgress;
use crate::archive::{PRODUCT_FILE, verify_product};
use crate::install::stage_in;

const DITTO: &str = "/usr/bin/ditto";
const CODESIGN: &str = "/usr/bin/codesign";
const STAGED_ZIP: &str = "release.zip";
const EXPANDED: &str = "expanded";
const PREVIOUS: &str = "previous.app";

/// The two macOS tools the bundle install shells out to, behind a seam so the
/// swap and its rollback are testable with plain directories on any host.
pub(crate) trait BundleTools {
    /// `ditto -x -k <archive> <into>`
    fn expand(&self, archive: &Path, into: &Path) -> Result<()>;
    /// `codesign --verify --deep --strict <bundle>`
    fn verify_signature(&self, bundle: &Path) -> Result<()>;
}

pub(crate) struct SystemTools;

impl BundleTools for SystemTools {
    fn expand(&self, archive: &Path, into: &Path) -> Result<()> {
        run(DITTO, &["-x", "-k"], &[archive, into])
    }

    fn verify_signature(&self, bundle: &Path) -> Result<()> {
        run(CODESIGN, &["--verify", "--deep", "--strict"], &[bundle])
    }
}

fn run(program: &str, flags: &[&str], paths: &[&Path]) -> Result<()> {
    let status = Command::new(program)
        .args(flags)
        .args(paths)
        .status()
        .with_context(|| format!("run {program}"))?;
    ensure!(status.success(), "{program} reported {status}");
    Ok(())
}

/// Where the new bundle goes and what it has to say about itself.
pub(crate) struct BundleInstall<'a> {
    /// The installed `<Name>.app` the new bundle replaces.
    pub bundle: &'a Path,
    /// The bundle directory name without `.app`, as the release zip carries
    /// it: the app's display name, such as `Subspace (Beta)`.
    pub bundle_name: &'a str,
    pub binary_name: &'a str,
    pub product_id: &'a str,
    pub bundle_id: &'a str,
    pub version: &'a str,
}

/// Installs `archive` over the bundle named in `install`.
#[cfg(test)]
pub(crate) fn install_bundle(
    install: &BundleInstall<'_>,
    archive: &[u8],
    tools: &dyn BundleTools,
) -> Result<()> {
    install_bundle_with_progress(install, archive, tools, &|_| {})
}

pub(crate) fn install_bundle_with_progress(
    install: &BundleInstall<'_>,
    archive: &[u8],
    tools: &dyn BundleTools,
    progress: &dyn Fn(UpdateProgress),
) -> Result<()> {
    let parent = install
        .bundle
        .parent()
        .context("the app bundle has no parent")?;
    let stage = stage_in(parent)?;
    progress(UpdateProgress::Extracting);
    let expanded = expand_archive(stage.path(), archive, tools)?;
    let staged = checked_bundle(install, &expanded, tools, progress)?;
    progress(UpdateProgress::Installing);
    swap(&staged, install.bundle, &stage.path().join(PREVIOUS))
}

fn expand_archive(stage: &Path, archive: &[u8], tools: &dyn BundleTools) -> Result<PathBuf> {
    let zip = stage.join(STAGED_ZIP);
    fs::write(&zip, archive).with_context(|| format!("write {}", zip.display()))?;
    let into = stage.join(EXPANDED);
    fs::create_dir(&into)?;
    tools.expand(&zip, &into)?;
    Ok(into)
}

/// The expanded `<Name>.app`, after its executable, `product.json` and code
/// signature are checked. Nothing is swapped until all three pass.
fn checked_bundle(
    install: &BundleInstall<'_>,
    expanded: &Path,
    tools: &dyn BundleTools,
    progress: &dyn Fn(UpdateProgress),
) -> Result<PathBuf> {
    let bundle = expanded.join(format!("{}.app", install.bundle_name));
    let executable = bundle
        .join("Contents")
        .join("MacOS")
        .join(install.binary_name);
    ensure!(
        executable.is_file(),
        "the release bundle has no Contents/MacOS/{}",
        install.binary_name
    );
    let product = bundle.join("Contents").join("Resources").join(PRODUCT_FILE);
    let bytes = fs::read(&product)
        .with_context(|| format!("the release bundle has no Contents/Resources/{PRODUCT_FILE}"))?;
    verify_product(
        &bytes,
        install.product_id,
        install.version,
        Some(install.bundle_id),
    )?;
    progress(UpdateProgress::VerifyingSignature);
    tools.verify_signature(&bundle)?;
    Ok(bundle)
}

/// Moves the installed bundle to `previous` and the new one into its place,
/// putting the old one back when the second rename fails. `previous` is in the
/// staging directory, so the old bundle goes away with the stage.
fn swap(new: &Path, bundle: &Path, previous: &Path) -> Result<()> {
    fs::rename(bundle, previous).with_context(|| format!("move {} aside", bundle.display()))?;
    match fs::rename(new, bundle) {
        Ok(()) => Ok(()),
        Err(error) => Err(restored(error, previous, bundle)),
    }
}

fn restored(error: std::io::Error, previous: &Path, bundle: &Path) -> anyhow::Error {
    let what = format!("install {}", bundle.display());
    match fs::rename(previous, bundle) {
        Ok(()) => anyhow::Error::new(error).context(what),
        Err(failure) => anyhow::anyhow!(
            "{what}: {error}; move {} back by hand: {failure}",
            previous.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::path::{Path, PathBuf};

    use anyhow::Result;

    use super::{BundleInstall, BundleTools, install_bundle, install_bundle_with_progress, swap};
    use crate::UpdateProgress;
    use crate::test_archive::product_json;

    const BINARY: &str = "subspace";
    const BUNDLE_ID: &str = "com.example.subspace";

    /// A seam that writes a plain directory tree instead of calling `ditto`,
    /// so the swap and its rollback run on any host. `break_before_swap`
    /// removes the checked bundle after the signature check, which is the
    /// only way the final rename can fail without a privileged filesystem.
    struct FakeTools {
        version: String,
        executable: bool,
        product: bool,
        signature: Result<(), &'static str>,
        break_before_swap: bool,
        /// The `.app` directory name the archive carries.
        bundle_name: &'static str,
    }

    impl Default for FakeTools {
        fn default() -> Self {
            Self {
                version: "1.2.0".into(),
                executable: true,
                product: true,
                signature: Ok(()),
                break_before_swap: false,
                bundle_name: "Subspace",
            }
        }
    }

    impl BundleTools for FakeTools {
        fn expand(&self, archive: &Path, into: &Path) -> Result<()> {
            assert!(archive.is_file(), "the zip must be staged before ditto");
            let bundle = into.join(format!("{}.app", self.bundle_name));
            if self.executable {
                let macos = bundle.join("Contents").join("MacOS");
                fs::create_dir_all(&macos)?;
                fs::write(macos.join(BINARY), b"new bundle binary")?;
            }
            if self.product {
                let resources = bundle.join("Contents").join("Resources");
                fs::create_dir_all(&resources)?;
                fs::write(resources.join("product.json"), product_json(&self.version))?;
            }
            Ok(())
        }

        fn verify_signature(&self, bundle: &Path) -> Result<()> {
            if self.break_before_swap {
                fs::remove_dir_all(bundle)?;
            }
            self.signature
                .map_err(|message| anyhow::anyhow!("codesign reported {message}"))
        }
    }

    fn installed() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("staging root");
        let bundle = directory.path().join("Subspace.app");
        let macos = bundle.join("Contents").join("MacOS");
        fs::create_dir_all(&macos).expect("installed bundle");
        fs::write(macos.join(BINARY), b"old bundle binary").expect("installed binary");
        (directory, bundle)
    }

    fn request<'a>(bundle: &'a Path, version: &'a str) -> BundleInstall<'a> {
        BundleInstall {
            bundle,
            bundle_name: "Subspace",
            binary_name: BINARY,
            product_id: "subspace",
            bundle_id: BUNDLE_ID,
            version,
        }
    }

    #[test]
    fn a_checked_bundle_replaces_the_installed_one_and_drops_the_old_one() {
        let (directory, bundle) = installed();
        install_bundle(&request(&bundle, "1.2.0"), b"zip", &FakeTools::default()).unwrap();
        let executable = bundle.join("Contents").join("MacOS").join(BINARY);
        assert_eq!(fs::read(executable).unwrap(), b"new bundle binary");
        let left: Vec<String> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["Subspace.app".to_owned()]);
    }

    #[test]
    fn a_type_update_keeps_the_type_bundle_name_and_location() {
        let directory = tempfile::tempdir().expect("staging root");
        let bundle = directory.path().join("Subspace (Beta).app");
        fs::create_dir_all(bundle.join("Contents").join("MacOS")).expect("installed bundle");
        let mut install = request(&bundle, "1.2.0");
        install.bundle_name = "Subspace (Beta)";
        let stable_archive = FakeTools::default();
        assert!(install_bundle(&install, b"zip", &stable_archive).is_err());
        let beta_archive = FakeTools {
            bundle_name: "Subspace (Beta)",
            ..FakeTools::default()
        };
        install_bundle(&install, b"zip", &beta_archive).unwrap();
        let executable = bundle.join("Contents").join("MacOS").join(BINARY);
        assert_eq!(fs::read(executable).unwrap(), b"new bundle binary");
        let left: Vec<String> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["Subspace (Beta).app".to_owned()]);
    }

    #[test]
    fn bundle_progress_reports_signature_before_installing() {
        let (_directory, bundle) = installed();
        let events = RefCell::new(Vec::new());
        install_bundle_with_progress(
            &request(&bundle, "1.2.0"),
            b"zip",
            &FakeTools::default(),
            &|event| {
                let executable = bundle.join("Contents/MacOS").join(BINARY);
                assert_eq!(fs::read(executable).unwrap(), b"old bundle binary");
                events.borrow_mut().push(event);
            },
        )
        .unwrap();
        assert_eq!(
            events.into_inner(),
            vec![
                UpdateProgress::Extracting,
                UpdateProgress::VerifyingSignature,
                UpdateProgress::Installing,
            ]
        );
    }

    #[test]
    fn failed_signature_progress_never_reports_installing() {
        let (_directory, bundle) = installed();
        let events = RefCell::new(Vec::new());
        let tools = FakeTools {
            signature: Err("broken seal"),
            ..FakeTools::default()
        };
        let result =
            install_bundle_with_progress(&request(&bundle, "1.2.0"), b"zip", &tools, &|event| {
                events.borrow_mut().push(event)
            });
        assert!(result.is_err());
        assert_eq!(
            events.into_inner(),
            vec![
                UpdateProgress::Extracting,
                UpdateProgress::VerifyingSignature
            ]
        );
        assert_eq!(
            fs::read(bundle.join("Contents/MacOS").join(BINARY)).unwrap(),
            b"old bundle binary"
        );
    }

    #[test]
    fn a_failed_swap_puts_the_previous_bundle_back() {
        let (directory, bundle) = installed();
        let tools = FakeTools {
            break_before_swap: true,
            ..FakeTools::default()
        };
        let error = install_bundle(&request(&bundle, "1.2.0"), b"zip", &tools).unwrap_err();
        assert!(error.to_string().contains("install"), "{error:#}");
        let executable = bundle.join("Contents").join("MacOS").join(BINARY);
        assert_eq!(fs::read(executable).unwrap(), b"old bundle binary");
        let left: Vec<String> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["Subspace.app".to_owned()]);
    }

    #[test]
    fn a_bundle_that_fails_a_check_is_never_swapped_in() {
        let cases = [
            (
                FakeTools {
                    executable: false,
                    ..FakeTools::default()
                },
                "Contents/MacOS",
            ),
            (
                FakeTools {
                    product: false,
                    ..FakeTools::default()
                },
                "Contents/Resources",
            ),
            (
                FakeTools {
                    version: "9.9.9".into(),
                    ..FakeTools::default()
                },
                "not 1.2.0",
            ),
            (
                FakeTools {
                    signature: Err("a broken seal"),
                    ..FakeTools::default()
                },
                "codesign",
            ),
        ];
        for (tools, expected) in cases {
            let (_directory, bundle) = installed();
            let error = install_bundle(&request(&bundle, "1.2.0"), b"zip", &tools).unwrap_err();
            assert!(
                format!("{error:#}").contains(expected),
                "expected {expected:?} in {error:#}"
            );
            let executable = bundle.join("Contents").join("MacOS").join(BINARY);
            assert_eq!(fs::read(executable).unwrap(), b"old bundle binary");
        }
    }

    #[test]
    fn a_bundle_with_another_identifier_is_refused() {
        let (_directory, bundle) = installed();
        let mut install = request(&bundle, "1.2.0");
        install.bundle_id = "com.example.other";
        let error = install_bundle(&install, b"zip", &FakeTools::default()).unwrap_err();
        assert!(
            format!("{error:#}").contains("bundle identifier"),
            "{error:#}"
        );
    }

    #[test]
    fn the_swap_rolls_back_when_the_new_directory_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let bundle = directory.path().join("App.app");
        fs::create_dir(&bundle).unwrap();
        fs::write(bundle.join("marker"), b"old").unwrap();
        let previous = directory.path().join("previous.app");
        let error = swap(&directory.path().join("missing.app"), &bundle, &previous).unwrap_err();
        assert!(error.to_string().contains("install"), "{error}");
        assert_eq!(fs::read(bundle.join("marker")).unwrap(), b"old");
        assert!(!previous.exists());
    }
}
