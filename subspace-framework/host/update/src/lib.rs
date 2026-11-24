//! Authenticated, checksum-verified updates for Subspace Framework apps.
//!
//! An app declares an update source in its product descriptor; this crate
//! turns that into a release check, a verified download and an install that
//! leaves the previous version in place when anything fails. The modules are
//! the steps: [`github`] talks to the API, [`archive`] names and unpacks an
//! asset, [`install`] replaces an executable, [`bundle`] swaps a macOS app
//! bundle, [`token`] resolves the credential, and [`cli`] is the `upgrade`
//! command.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use subspace_core::credentials::{ProcessEnvironment, SystemRunner};
use subspace_product::{UpdateSource, ValidatedProduct};

mod archive;
mod bundle;
mod cli;
mod github;
mod install;
mod token;
mod version;

#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod test_archive;

pub use cli::run_cli;
pub use install::cleanup_previous;

use archive::{Wanted, binary_entry_name, extract, verify_checksum, verify_product, wanted_asset};
use bundle::{BundleInstall, SystemTools, install_bundle_with_progress};
use github::{API_URL_ENV, Client, api_base, no_release_message};
use install::{replace_executable, require_own_executable_name};
use token::Token;
use version::{is_newer, require_release_version};

/// One published release of an app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// The full Git tag, such as `subspace-desktop-v1.2.0`.
    pub tag: String,
    /// The `X.Y.Z` part of the tag.
    pub version: String,
    /// The release page, for an About dialog or a check report.
    pub html_url: String,
    /// Asset name to GitHub API URL. Private so no caller can download an
    /// asset outside this crate's checks.
    assets: BTreeMap<String, String>,
}

/// The result of checking this product's release channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    Newer(Release),
    UpToDate,
    NoRelease { repository: String },
}

impl CheckOutcome {
    pub fn newer(&self) -> Option<&Release> {
        match self {
            Self::Newer(release) => Some(release),
            _ => None,
        }
    }
}

/// What an install replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub previous_version: String,
    pub version: String,
    pub path: PathBuf,
}

/// The current install phase. Download bytes describe only the release archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateProgress {
    Preparing,
    Downloading { received: u64, total: Option<u64> },
    DownloadingChecksum,
    VerifyingChecksum,
    Extracting,
    VerifyingSignature,
    Installing,
}

/// Where the token comes from. The process environment in an app; a fixed
/// value in this crate's tests, so no test reads the real environment or runs
/// `gh`.
#[derive(Clone, Debug)]
enum Auth {
    Process,
    #[cfg(test)]
    Fixed(Token),
}

/// The running app build: what a release has to say about itself, and what
/// the installed file is called.
#[derive(Clone, Debug)]
struct App {
    id: String,
    binary_name: String,
    /// The display name, which is also the `.app` directory name in a
    /// release bundle archive.
    display_name: String,
    bundle_id: String,
    version: String,
}

/// The updater for one app build.
#[derive(Clone, Debug)]
pub struct Updater {
    source: UpdateSource,
    app: App,
    api: String,
    auth: Auth,
    account: Option<String>,
}

impl Updater {
    /// `None` when the app has no update source, so a caller cannot offer
    /// update controls it could not honour.
    pub fn for_product(product: &ValidatedProduct, current_version: &str) -> Option<Self> {
        if subspace_product::is_integration_version(current_version) {
            return None;
        }
        Some(Self {
            source: product.update.clone()?,
            app: App {
                id: product.id.as_str().to_owned(),
                binary_name: product.binary_name.as_str().to_owned(),
                display_name: product.display_name.clone(),
                bundle_id: product.bundle_id.clone(),
                version: current_version.to_owned(),
            },
            api: api_base(std::env::var(API_URL_ENV).ok()),
            auth: Auth::Process,
            account: None,
        })
    }

    /// The GitHub account chosen in Settings for this operation.
    pub fn with_account(mut self, login: Option<String>) -> Self {
        self.account = login;
        self
    }

    pub fn account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    pub fn current_version(&self) -> &str {
        &self.app.version
    }

    /// Whether a release is newer, current, or has yet to be published.
    pub fn check(&self) -> Result<CheckOutcome> {
        let newest = self.client()?.newest()?;
        Ok(match newest {
            Some(release)
                if is_newer(self.source.channel, &release.version, &self.app.version)
                    && self.offers(&release) =>
            {
                CheckOutcome::Newer(release)
            }
            Some(_) => CheckOutcome::UpToDate,
            None => CheckOutcome::NoRelease {
                repository: self.source.repository.clone(),
            },
        })
    }

    /// Whether `release` is one this build installs: any release, or for an
    /// exclusive source only one that publishes this build's own archive and
    /// its checksum, so a variant never offers, nor installs, another
    /// build's archive.
    fn offers(&self, release: &Release) -> bool {
        !self.source.exclusive
            || current_executable().is_ok_and(|executable| self.publishes_own(release, &executable))
    }

    /// Whether `release` publishes the archive, and its checksum, that this
    /// build at `executable` installs.
    fn publishes_own(&self, release: &Release, executable: &Path) -> bool {
        wanted_asset(&self.source, &release.version, executable).is_ok_and(|wanted| {
            let name = wanted.name();
            release.assets.contains_key(name)
                && release.assets.contains_key(&format!("{name}.sha256"))
        })
    }

    /// The newest release in this build's channel, whether or not it is newer.
    pub fn latest(&self) -> Result<Release> {
        self.client()?
            .newest()?
            .with_context(|| no_release_message(&self.source))
    }

    /// One exact release by version, including an older one.
    pub fn find(&self, version: &str) -> Result<Release> {
        let version = require_release_version(self.source.channel, version)?;
        self.client()?.find(&version)
    }

    /// Downloads, verifies and installs `release` over the running
    /// executable. Processes that are already running keep the old version.
    pub fn install(&self, release: &Release) -> Result<Installed> {
        self.install_at(release, &current_executable()?)
    }

    /// Installs with synchronous progress callbacks on the calling thread.
    /// Call this from a background thread when updating a user interface.
    pub fn install_with_progress(
        &self,
        release: &Release,
        progress: impl Fn(UpdateProgress),
    ) -> Result<Installed> {
        self.install_at_with_progress(release, &current_executable()?, &progress)
    }

    fn install_at_with_progress(
        &self,
        release: &Release,
        executable: &Path,
        progress: &dyn Fn(UpdateProgress),
    ) -> Result<Installed> {
        progress(UpdateProgress::Preparing);
        require_own_executable_name(executable, &self.app.binary_name)?;
        let wanted = wanted_asset(&self.source, &release.version, executable)?;
        let client = self.client()?;
        // The update popup may have been open while release assets changed.
        let release = client.find(&release.version)?;
        Job {
            app: &self.app,
            client,
            release: &release,
            executable,
        }
        .run_with_progress(&wanted, progress)
    }

    /// An explicit path keeps tests from replacing their own executable.
    fn install_at(&self, release: &Release, executable: &Path) -> Result<Installed> {
        self.install_at_with_progress(release, executable, &|_| {})
    }

    /// A session with the token resolved once, however many requests the
    /// operation makes.
    fn client(&self) -> Result<Client<'_>> {
        Ok(Client::new(&self.api, &self.source, self.token()?))
    }

    fn token(&self) -> Result<Token> {
        match &self.auth {
            Auth::Process => token::resolve(
                &self.source,
                self.account.as_deref(),
                &ProcessEnvironment,
                &token::signed_in_token,
                &SystemRunner,
            ),
            #[cfg(test)]
            Auth::Fixed(token) => Ok(token.clone()),
        }
    }

    /// A fixed API base and token for this crate's local-server tests.
    #[cfg(test)]
    fn against(mut self, api: &str, token: &str) -> Self {
        self.api = api.to_owned();
        self.auth = Auth::Fixed(Token::new(token));
        self
    }
}

fn current_executable() -> Result<PathBuf> {
    let executable = std::env::current_exe().context("locate the executable to update")?;
    fs::canonicalize(&executable).context("locate the executable to update")
}

/// One install: which app, which release, over which executable, through
/// which session.
struct Job<'a> {
    app: &'a App,
    client: Client<'a>,
    release: &'a Release,
    executable: &'a Path,
}

impl Job<'_> {
    #[cfg(test)]
    fn run(&self, wanted: &Wanted) -> Result<Installed> {
        self.run_with_progress(wanted, &|_| {})
    }

    fn run_with_progress(
        &self,
        wanted: &Wanted,
        progress: &dyn Fn(UpdateProgress),
    ) -> Result<Installed> {
        let archive = self.verified_asset(wanted.name(), progress)?;
        self.apply(wanted, &archive, progress)?;
        Ok(Installed {
            previous_version: self.app.version.clone(),
            version: self.release.version.clone(),
            path: self.executable.to_owned(),
        })
    }

    /// The asset and its `<asset>.sha256`, with the digest checked before
    /// anything is written.
    fn verified_asset(&self, name: &str, progress: &dyn Fn(UpdateProgress)) -> Result<Vec<u8>> {
        anyhow::ensure!(
            self.release.assets.contains_key(name)
                && self.release.assets.contains_key(&format!("{name}.sha256")),
            "release {} has no {name} archive or checksum yet; check for updates again later. Nothing was installed",
            self.release.tag
        );
        let bytes = self
            .client
            .asset_with_progress(self.release, name, progress)?;
        progress(UpdateProgress::DownloadingChecksum);
        let checksum = self.client.asset(self.release, &format!("{name}.sha256"))?;
        progress(UpdateProgress::VerifyingChecksum);
        verify_checksum(&checksum, name, &bytes)?;
        Ok(bytes)
    }

    fn apply(
        &self,
        wanted: &Wanted,
        archive: &[u8],
        progress: &dyn Fn(UpdateProgress),
    ) -> Result<()> {
        match wanted {
            Wanted::Plain { zip, .. } => self.plain(archive, *zip, progress),
            Wanted::Bundle { .. } => self.bundle(archive, progress),
        }
    }

    fn plain(&self, archive: &[u8], zip: bool, progress: &dyn Fn(UpdateProgress)) -> Result<()> {
        let binary = binary_entry_name(&self.app.binary_name, zip);
        progress(UpdateProgress::Extracting);
        let contents = extract(archive, zip, &binary)?;
        verify_product(&contents.product, &self.app.id, &self.release.version, None)?;
        progress(UpdateProgress::Installing);
        replace_executable(self.executable, &contents.binary, cfg!(windows))
    }

    fn bundle(&self, archive: &[u8], progress: &dyn Fn(UpdateProgress)) -> Result<()> {
        let root = archive::bundle_root(self.executable)
            .context("the executable is not inside an app bundle")?;
        let install = BundleInstall {
            bundle: root,
            bundle_name: &self.app.display_name,
            binary_name: &self.app.binary_name,
            product_id: &self.app.id,
            bundle_id: &self.app.bundle_id,
            version: &self.release.version,
        };
        install_bundle_with_progress(&install, archive, &SystemTools, progress)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use subspace_product::{ValidatedProduct, fixture};

    use super::{Release, Updater};

    pub(crate) fn known_product() -> ValidatedProduct {
        fixture::products()
            .expect("the known products validate")
            .into_iter()
            .find(|product| product.id.as_str() == "subspace")
            .expect("the product exists")
    }

    #[test]
    fn the_known_app_has_an_updater_and_reports_the_running_version() {
        for product in fixture::products().expect("the known products validate") {
            let updater = Updater::for_product(&product, "1.1.0")
                .unwrap_or_else(|| panic!("{} has an update source", product.id.as_str()));
            assert_eq!(updater.current_version(), "1.1.0");
        }
    }

    /// A GUI check runs off the UI thread and hands the release back, so a
    /// `Release` has to clone, cross threads and compare.
    #[test]
    fn a_release_crosses_threads_for_a_background_check() {
        let release = Release {
            tag: "subspace-desktop-v1.2.0".into(),
            version: "1.2.0".into(),
            html_url: "https://example.test/release".into(),
            assets: BTreeMap::new(),
        };
        let sent = release.clone();
        let received = std::thread::spawn(move || sent)
            .join()
            .expect("the release crosses threads");
        assert_eq!(received, release);
    }

    /// A variant with its own asset names is offered only a release that
    /// publishes its archive; the ordinary build is offered every newer
    /// release, as before.
    #[test]
    fn an_exclusive_build_is_offered_only_releases_with_its_own_archive() {
        let executable = std::path::Path::new("/opt/example/subspace");
        let release = |names: &[String]| Release {
            tag: "subspace-desktop-v9.0.0".into(),
            version: "9.0.0".into(),
            html_url: "https://example.test/release".into(),
            assets: names
                .iter()
                .map(|name| (name.clone(), format!("https://api.example.test/{name}")))
                .collect(),
        };
        let name = |updater: &Updater| {
            super::wanted_asset(&updater.source, "9.0.0", executable)
                .unwrap()
                .name()
                .to_owned()
        };
        let standard = Updater::for_product(&known_product(), "1.0.0").unwrap();
        let mut variant = standard.clone();
        variant.source.archive_prefix = "subspace-no-api".into();
        variant.source.exclusive = true;
        let (own, theirs) = (name(&variant), name(&standard));
        assert!(own.starts_with("subspace-no-api-9.0.0-") && theirs.starts_with("subspace-9.0.0-"));
        let only_standard = release(&[theirs.clone(), format!("{theirs}.sha256")]);
        let both = release(&[
            theirs.clone(),
            format!("{theirs}.sha256"),
            own.clone(),
            format!("{own}.sha256"),
        ]);
        assert!(!variant.publishes_own(&only_standard, executable));
        assert!(variant.publishes_own(&both, executable));
        assert!(
            !variant.publishes_own(&release(std::slice::from_ref(&own)), executable),
            "no checksum"
        );
        // The ordinary build is offered a newer release whatever it holds.
        assert!(standard.offers(&release(&[])));
        assert!(standard.offers(&only_standard));
    }

    #[test]
    fn an_app_without_an_update_source_has_no_updater() {
        let mut product = known_product();
        product.update = None;
        assert!(Updater::for_product(&product, "1.1.0").is_none());
    }

    #[test]
    fn integration_versions_cannot_create_an_updater_even_with_a_stable_source() {
        assert!(
            Updater::for_product(
                &known_product(),
                "0.3.0-feat-subspace-desktop.202611151345.gabc1234"
            )
            .is_none()
        );
    }

    #[test]
    fn an_installed_version_must_be_of_the_release_form_before_any_request() {
        let updater = Updater::for_product(&known_product(), "1.1.0")
            .expect("the standalone product has an update source")
            .against("http://127.0.0.1:1/unused", "fixture-token");
        let error = updater.find("not-a-version").unwrap_err();
        assert!(error.to_string().contains("1.2.3"), "{error}");
    }

    #[test]
    fn a_nightly_updater_refuses_a_pinned_stable_version_before_any_request() {
        let product = subspace_product::validate_product(
            &subspace_product::fixture::descriptor_for(subspace_product::Channel::Nightly),
        )
        .expect("the nightly descriptor validates");
        let updater = Updater::for_product(&product, "1.10.2-nightly.202611010200.ga43a333")
            .expect("the nightly product has an update source")
            .against("http://127.0.0.1:1/unused", "fixture-token");
        let error = updater.find("1.10.1").unwrap_err();
        assert!(
            error.to_string().contains("only nightly versions"),
            "{error}"
        );
    }

    #[test]
    fn an_executable_a_package_manager_installed_is_refused_before_any_download() {
        let directory = tempfile::tempdir().expect("a temporary installation folder");
        let executable = directory.path().join("subspace-packaged");
        std::fs::write(&executable, b"feed installation").expect("write the executable");
        let updater = Updater::for_product(&known_product(), "1.1.0")
            .expect("the standalone product has an update source")
            .against("http://127.0.0.1:1/unused", "fixture-token");
        let release = Release {
            tag: "subspace-desktop-v1.2.0".into(),
            version: "1.2.0".into(),
            html_url: String::new(),
            assets: BTreeMap::new(),
        };
        let error = updater.install_at(&release, &executable).unwrap_err();
        assert!(error.to_string().contains("package manager"), "{error}");
        assert_eq!(std::fs::read(&executable).unwrap(), b"feed installation");
    }

    #[test]
    fn an_asset_url_outside_the_api_base_is_refused_before_the_token_is_sent() {
        let updater = Updater::for_product(&known_product(), "1.1.0")
            .expect("the standalone product has an update source")
            .against("https://api.example.test", "fixture-token");
        let release = Release {
            tag: "subspace-desktop-v1.2.0".into(),
            version: "1.2.0".into(),
            html_url: String::new(),
            assets: BTreeMap::from([(
                "asset".to_owned(),
                "https://elsewhere.invalid/asset".to_owned(),
            )]),
        };
        let client = updater.client().expect("a fixed token needs no lookup");
        let error = client.asset(&release, "asset").unwrap_err();
        assert!(
            error.to_string().contains("not on the GitHub API"),
            "{error}"
        );
    }
}
