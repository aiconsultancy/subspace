//! GitHub release metadata and authenticated asset downloads (REQ-02,
//! REQ-03). The token goes to the API base only, and `ureq` drops the
//! `Authorization` header when GitHub redirects an asset to storage.

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use subspace_product::UpdateSource;

use crate::archive::{MAX_DOWNLOAD, MAX_METADATA, read_limited_with_progress};
use crate::token::Token;
use crate::version::{is_newer, release_version};
use crate::{Release, UpdateProgress};

pub(crate) const DEFAULT_API: &str = "https://api.github.com";
/// Debug builds only: the API base to use instead of [`DEFAULT_API`]. The
/// GitHub client reads the same variable, so a capture build's update check
/// reaches the fictional GitHub server rather than the real one.
pub(crate) const API_URL_ENV: &str = "SUBSPACE_GITHUB_API_URL";

const HTTP_TIMEOUT: Duration = Duration::from_secs(180);
const RELEASES_PER_PAGE: usize = 100;
const RELEASE_PAGE_LIMIT: u32 = 1000;
const API_VERSION: &str = "2022-11-28";
const USER_AGENT: &str = "subspace-updater";

/// Which `Accept` header and size limit a request uses.
#[derive(Clone, Copy)]
pub(crate) enum Payload {
    Metadata,
    Asset,
}

impl Payload {
    fn accept(self) -> &'static str {
        match self {
            Self::Metadata => "application/vnd.github+json",
            Self::Asset => "application/octet-stream",
        }
    }

    fn limit(self) -> u64 {
        match self {
            Self::Metadata => MAX_METADATA,
            Self::Asset => MAX_DOWNLOAD,
        }
    }
}

/// One authenticated GitHub session. The token is resolved before the
/// session is built, so an operation resolves it once however many requests
/// it makes.
pub(crate) struct Client<'a> {
    api: &'a str,
    source: &'a UpdateSource,
    token: Token,
    http: ureq::Agent,
}

impl<'a> Client<'a> {
    pub(crate) fn new(api: &'a str, source: &'a UpdateSource, token: Token) -> Self {
        Self {
            api,
            source,
            token,
            http: agent(),
        }
    }

    /// Fetches `url`, which must be on the API base so the token never
    /// reaches another host.
    pub(crate) fn get(&self, url: &str, payload: Payload) -> Result<Vec<u8>> {
        self.get_with_progress(url, payload, &|_| {})
    }

    fn get_with_progress(
        &self,
        url: &str,
        payload: Payload,
        progress: &dyn Fn(UpdateProgress),
    ) -> Result<Vec<u8>> {
        ensure!(
            url.starts_with(&format!("{}/", self.api)),
            "release URL is not on the GitHub API"
        );
        let mut response = self.send(url, payload)?;
        let total = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        read_limited_with_progress(
            response.body_mut().as_reader(),
            payload.limit(),
            &|received| progress(UpdateProgress::Downloading { received, total }),
        )
    }

    /// Downloads a named release asset through its API URL, which is the
    /// only URL a token can read in a private repository.
    pub(crate) fn asset(&self, release: &Release, name: &str) -> Result<Vec<u8>> {
        self.asset_with_progress(release, name, &|_| {})
    }

    pub(crate) fn asset_with_progress(
        &self,
        release: &Release,
        name: &str,
        progress: &dyn Fn(UpdateProgress),
    ) -> Result<Vec<u8>> {
        let url = release
            .assets
            .get(name)
            .with_context(|| format!("release {} has no {name} asset", release.tag))?;
        self.get_with_progress(url, Payload::Asset, progress)
    }

    /// The highest release of this app in its channel across every page, or
    /// `None` when the repository has no such release yet.
    pub(crate) fn newest(&self) -> Result<Option<Release>> {
        let mut best = None;
        for page in 1..=RELEASE_PAGE_LIMIT {
            let url = format!(
                "{}/repos/{}/releases?per_page={RELEASES_PER_PAGE}&page={page}",
                self.api, self.source.repository
            );
            let releases: Vec<GithubRelease> =
                serde_json::from_slice(&self.get(&url, Payload::Metadata)?)
                    .context("decode the GitHub releases")?;
            let last_page = releases.len() < RELEASES_PER_PAGE;
            best = newest_of(best, releases, self.source);
            if last_page {
                return Ok(best);
            }
        }
        bail!(
            "the GitHub release search exceeded {RELEASE_PAGE_LIMIT} pages; select --version explicitly"
        )
    }

    /// One exact release by version, including an older one.
    pub(crate) fn find(&self, version: &str) -> Result<Release> {
        let tag = format!("{}{version}", self.source.tag_prefix);
        let url = format!(
            "{}/repos/{}/releases/tags/{}",
            self.api,
            self.source.repository,
            encoded_tag(&tag)
        );
        let value = serde_json::from_slice(&self.get(&url, Payload::Metadata)?)
            .context("decode the GitHub release")?;
        let release = candidate_for(value, self.source).with_context(|| {
            format!(
                "the requested release is not a {} release of this app",
                self.source.channel.as_str()
            )
        })?;
        ensure!(
            release.version == version,
            "GitHub returned a different release than requested"
        );
        Ok(release)
    }

    fn send(&self, url: &str, payload: Payload) -> Result<ureq::http::Response<ureq::Body>> {
        let result = self
            .http
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token.expose()))
            .header("Accept", payload.accept())
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("User-Agent", USER_AGENT)
            .call();
        match result {
            Ok(response) => Ok(response),
            Err(ureq::Error::StatusCode(401)) => bail!(
                "GitHub rejected the token; run gh auth login or replace {}",
                self.source.token_variable
            ),
            Err(ureq::Error::StatusCode(404)) => bail!(
                "GitHub returned 404: the release does not exist, or the token cannot read {}; authorize the token for your organization's SSO",
                self.source.repository
            ),
            Err(error) => Err(error).context("reach GitHub"),
        }
    }
}

/// The shared agent: the platform certificate store, the environment's proxy
/// settings, and a long timeout because a release asset can be large.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .proxy(ureq::Proxy::try_from_env())
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .unversioned_rustls_crypto_provider(Arc::new(
                    rustls::crypto::aws_lc_rs::default_provider(),
                ))
                .build(),
        )
        .build()
        .into()
}

fn encoded_tag(tag: &str) -> String {
    tag.replace('/', "%2F")
}

/// [`DEFAULT_API`], unless a debug build names the numeric loopback fixture in `value`
/// (the [`API_URL_ENV`] variable).
pub(crate) fn api_base(value: Option<String>) -> String {
    value
        .filter(|_| cfg!(debug_assertions))
        .map(|value| value.trim().trim_end_matches('/').to_owned())
        .filter(|value| {
            value
                .strip_prefix("http://127.0.0.1:")
                .is_some_and(|port| port.parse::<u16>().is_ok_and(|port| port != 0))
        })
        .unwrap_or_else(|| DEFAULT_API.to_owned())
}

/// Why `upgrade` has nothing to select: no release in this build's channel.
pub(crate) fn no_release_message(source: &UpdateSource) -> String {
    format!(
        "{} has no {} release tagged {}X.Y.Z",
        source.repository,
        source.channel.as_str(),
        source.tag_prefix
    )
}

#[derive(Deserialize)]
pub(crate) struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    /// The API URL. The browser download URL is deliberately ignored: it is
    /// not on the API base, so a private-repository download fails there.
    url: String,
}

/// A published, non-draft release of this app in the source's channel, or
/// `None`. Stable takes only releases without the pre-release flag; a type
/// takes only flagged pre-releases whose version has that type.
pub(crate) fn candidate_for(value: GithubRelease, source: &UpdateSource) -> Option<Release> {
    let version = value.tag_name.strip_prefix(&source.tag_prefix)?.to_owned();
    if value.draft
        || value.prerelease != source.channel.is_prerelease()
        || release_version(source.channel, &version).is_none()
    {
        return None;
    }
    Some(Release {
        tag: value.tag_name,
        version,
        html_url: value.html_url,
        assets: value
            .assets
            .into_iter()
            .map(|asset| (asset.name, asset.url))
            .collect(),
    })
}

fn newest_of(
    best: Option<Release>,
    page: Vec<GithubRelease>,
    source: &UpdateSource,
) -> Option<Release> {
    page.into_iter()
        .filter_map(|value| candidate_for(value, source))
        .fold(best, |best, release| match best {
            Some(best) if !is_newer(source.channel, &release.version, &best.version) => Some(best),
            _ => Some(release),
        })
}

#[cfg(test)]
mod tests {
    use subspace_product::{Channel, UpdateSource};

    use super::{DEFAULT_API, api_base, candidate_for, encoded_tag, newest_of, no_release_message};

    fn source() -> UpdateSource {
        UpdateSource {
            repository: "owner/name".into(),
            tag_prefix: "tools/app/v".into(),
            archive_prefix: "app".into(),
            app_bundle: "App".into(),
            token_variable: "APP_GITHUB_TOKEN".into(),
            channel: Channel::Stable,
        }
    }

    fn nightly_source() -> UpdateSource {
        UpdateSource {
            channel: Channel::Nightly,
            ..source()
        }
    }

    fn mixed_page() -> Vec<super::GithubRelease> {
        vec![
            release("tools/app/v1.10.2", false, false),
            release(
                "tools/app/v1.10.2-nightly.202611010200.ga43a333",
                false,
                true,
            ),
            release(
                "tools/app/v1.10.2-nightly.202611020200.gb0c1d2e",
                false,
                true,
            ),
            release(
                "tools/app/v1.10.2-nightly.202611030200.gc0ffee0",
                true,
                true,
            ),
            release("tools/app/v1.10.3-beta.202611010200.g0123456", false, true),
            release(
                "tools/app/v1.10.2-nightly.202611040200.gdeadbee",
                false,
                false,
            ),
        ]
    }

    #[test]
    fn a_nightly_build_selects_only_the_newest_published_nightly() {
        let best = newest_of(None, mixed_page(), &nightly_source());
        assert_eq!(
            best.map(|r| r.version),
            Some("1.10.2-nightly.202611020200.gb0c1d2e".to_owned())
        );
    }

    #[test]
    fn a_stable_build_never_selects_a_pre_release() {
        let best = newest_of(None, mixed_page(), &source());
        assert_eq!(best.map(|r| r.version), Some("1.10.2".to_owned()));
        let only_prereleases = mixed_page().into_iter().skip(1).collect();
        assert_eq!(newest_of(None, only_prereleases, &source()), None);
    }

    fn release(tag: &str, draft: bool, prerelease: bool) -> super::GithubRelease {
        let value = serde_json::json!({
            "tag_name": tag,
            "draft": draft,
            "prerelease": prerelease,
            "html_url": format!("https://example.test/{tag}"),
        });
        serde_json::from_value(value).expect("release metadata decodes")
    }

    #[test]
    fn only_stable_releases_with_this_apps_tag_prefix_are_candidates() {
        let cases = [
            ("tools/app/v1.2.3", false, false, true),
            ("tools/app/v1.2.3", true, false, false),
            ("tools/app/v1.2.3", false, true, false),
            ("tools/app/v1.2.3-beta", false, false, false),
            ("tools/app/v01.2.3", false, false, false),
            ("tools/app/v1.2", false, false, false),
            ("tools/other/v1.2.3", false, false, false),
            ("v1.2.3", false, false, false),
        ];
        for (tag, draft, prerelease, expected) in cases {
            let candidate = candidate_for(release(tag, draft, prerelease), &source());
            assert_eq!(candidate.is_some(), expected, "{tag}");
        }
        let accepted = candidate_for(release("tools/app/v1.2.3", false, false), &source()).unwrap();
        assert_eq!(accepted.version, "1.2.3");
        assert_eq!(accepted.tag, "tools/app/v1.2.3");
        assert_eq!(accepted.html_url, "https://example.test/tools/app/v1.2.3");
    }

    #[test]
    fn the_highest_version_wins_across_pages_and_ignored_entries() {
        let first = vec![
            release("tools/app/v1.9.0", false, false),
            release("tools/app/v2.0.0", true, false),
            release("other-tool/v9.9.9", false, false),
        ];
        let second = vec![
            release("tools/app/v1.10.0", false, false),
            release("tools/app/v1.10.1-rc1", false, false),
        ];
        let best = newest_of(None, first, &source());
        assert_eq!(best.as_ref().map(|r| r.version.as_str()), Some("1.9.0"));
        let best = newest_of(best, second, &source());
        assert_eq!(best.map(|r| r.version), Some("1.10.0".to_owned()));
    }

    #[test]
    fn a_page_with_no_candidate_keeps_the_previous_best() {
        let best = newest_of(
            None,
            vec![release("tools/app/v1.0.0", false, false)],
            &source(),
        );
        let kept = newest_of(
            best,
            vec![release("tools/app/v0.9.0", false, false)],
            &source(),
        );
        assert_eq!(kept.map(|r| r.version), Some("1.0.0".to_owned()));
    }

    #[test]
    fn a_debug_build_reads_the_api_base_from_the_variable() {
        let fixture = if cfg!(debug_assertions) {
            "http://127.0.0.1:9"
        } else {
            DEFAULT_API
        };
        assert_eq!(api_base(Some(" http://127.0.0.1:9/ ".into())), fixture);
        assert_eq!(api_base(None), DEFAULT_API);
        assert_eq!(api_base(Some(" / ".into())), DEFAULT_API);
        assert_eq!(
            api_base(Some("http://127.0.0.1:9@invalid.test".into())),
            DEFAULT_API
        );
        assert_eq!(api_base(Some("https://invalid.test".into())), DEFAULT_API);
    }

    #[test]
    fn the_no_release_message_names_the_repository_channel_and_tag_form() {
        assert_eq!(
            no_release_message(&nightly_source()),
            "owner/name has no nightly release tagged tools/app/vX.Y.Z"
        );
    }

    #[test]
    fn a_tag_with_slashes_is_url_encoded() {
        assert_eq!(encoded_tag("release/v1.2.0"), "release%2Fv1.2.0");
        assert_eq!(encoded_tag("v1.2.0"), "v1.2.0");
    }
}
