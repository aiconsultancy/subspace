//! Organization-authorized built package reads. No fallback, writes or Git process.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use subspace_core::http::Error;

use super::BoundRead;
use crate::{transport, validate};

mod download;
mod errors;
mod selection;
use errors::Read;
pub use newest_release as newest_package_release;
#[cfg(test)]
mod tests;

const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 10;
const JSON_BYTES: usize = 4 * 1024 * 1024;
const ZIP_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "camelCase")]
pub enum RepositoryRef {
    DefaultBranch,
    Branch(String),
    Tag(String),
    Commit(String),
}

#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct Organization {
    pub login: String,
}

#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct PackageRepository {
    pub full_name: String,
    pub default_branch: String,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ResolvedRepository {
    pub repository: String,
    pub reference: RepositoryRef,
    pub commit: String,
}

#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct ReleaseAsset {
    pub id: u64,
    pub name: String,
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct PackageRelease {
    pub id: u64,
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

impl BoundRead {
    /// Active memberships only; a public organization is not authorization.
    // qual:api
    pub fn organizations(&self) -> Result<Vec<Organization>, Error> {
        let rows: Vec<Value> =
            self.package_pages("/user/memberships/orgs?state=active", Read::MEMBERSHIP)?;
        rows.into_iter().map(selection::organization).collect()
    }

    /// Discover readable repositories after verifying active membership.
    // qual:api
    pub fn repositories(&self, organization: &str) -> Result<Vec<PackageRepository>, Error> {
        self.package_membership(organization)?;
        self.package_pages(
            &format!("/orgs/{organization}/repos?type=all&sort=full_name"),
            Read::REPOSITORY,
        )
    }

    /// Each operation rechecks membership and repository read access.
    // qual:api
    pub fn resolve_repository(
        &self,
        repository: &str,
        reference: &RepositoryRef,
    ) -> Result<ResolvedRepository, Error> {
        let repo = self.package_repository(repository)?;
        let reference = selection::effective_ref(reference, &repo.default_branch)?;
        let expression = selection::expression(&reference)?;
        let encoded: String = url::form_urlencoded::byte_serialize(expression.as_bytes()).collect();
        let value: Value = self.package_json(
            &format!("/repos/{repository}/commits/{encoded}"),
            Read::REFERENCE,
        )?;
        let commit = value["sha"]
            .as_str()
            .ok_or_else(|| errors::malformed(Read::REFERENCE))?;
        validate::check_sha(commit).map_err(|_| errors::malformed(Read::REFERENCE))?;
        Ok(ResolvedRepository {
            repository: repo.full_name,
            reference,
            commit: commit.into(),
        })
    }

    // qual:api
    pub fn package_releases(&self, repository: &str) -> Result<Vec<PackageRelease>, Error> {
        self.package_repository(repository)?;
        self.package_pages(&format!("/repos/{repository}/releases?"), Read::RELEASE)
    }

    pub fn package_release(&self, repository: &str, id: u64) -> Result<PackageRelease, Error> {
        self.package_repository(repository)?;
        if id == 0 {
            return Err(validate::invalid("Select a published release."));
        }
        self.package_json(&format!("/repos/{repository}/releases/{id}"), Read::RELEASE)
    }

    // qual:api
    pub fn download_repository(&self, repository: &str, commit: &str) -> Result<Vec<u8>, Error> {
        self.package_repository(repository)?;
        validate::check_sha(commit)?;
        // The archive API requires GitHub's JSON media type, even for ZIP bytes.
        self.package_download(
            &format!("/repos/{repository}/zipball/{commit}"),
            ZIP_BYTES,
            transport::JSON,
        )
    }

    // qual:api
    pub fn download_release(
        &self,
        repository: &str,
        id: u64,
        asset_name: &str,
        include_prereleases: bool,
    ) -> Result<Vec<u8>, Error> {
        let release = self.package_release(repository, id)?;
        let asset = selection::asset(&release, asset_name, include_prereleases)?;
        if asset.size > ZIP_BYTES as u64 {
            return Err(validate::invalid(
                "The ZIP asset exceeds the download limit.",
            ));
        }
        self.package_download(
            &format!("/repos/{repository}/releases/assets/{}", asset.id),
            ZIP_BYTES,
            "application/octet-stream",
        )
    }

    fn package_membership(&self, organization: &str) -> Result<(), Error> {
        validate::check_login(organization)?;
        let membership: Value = self.package_json(
            &format!("/user/memberships/orgs/{organization}"),
            Read::MEMBERSHIP,
        )?;
        selection::membership(&membership, organization)
    }

    fn package_repository(&self, repository: &str) -> Result<PackageRepository, Error> {
        let (owner, _) = validate::check_repo(repository)?;
        self.package_membership(&owner)?;
        let value: Value = self.package_json(&format!("/repos/{repository}"), Read::REPOSITORY)?;
        selection::repository(value, repository)
    }

    fn package_json<T: DeserializeOwned>(&self, path: &str, read: Read) -> Result<T, Error> {
        let response = self.package_request(path, transport::JSON, read)?;
        let bytes = transport::read_limited(response, JSON_BYTES)
            .map_err(|error| errors::body(error, read))?;
        serde_json::from_slice(&bytes).map_err(|_| errors::malformed(read))
    }

    fn package_pages<T: DeserializeOwned>(&self, path: &str, read: Read) -> Result<Vec<T>, Error> {
        let mut result = Vec::new();
        for page in 1..=MAX_PAGES {
            let separator = if path.contains('?') { '&' } else { '?' };
            let rows: Vec<T> = self.package_json(
                &format!("{path}{separator}per_page={PAGE_SIZE}&page={page}"),
                read,
            )?;
            let complete = rows.len() < PAGE_SIZE;
            result.extend(rows);
            if complete {
                return Ok(result);
            }
        }
        Err(validate::invalid(
            "GitHub discovery exceeds 1,000 results. Select a narrower source.",
        ))
    }
}

/// Select the newest published eligible release containing this exact ZIP name.
// qual:api
pub fn newest_release<'a>(
    releases: &'a [PackageRelease],
    asset_name: &str,
    include_prereleases: bool,
) -> Option<&'a PackageRelease> {
    releases
        .iter()
        .filter(|release| selection::asset(release, asset_name, include_prereleases).is_ok())
        .max_by(|left, right| {
            left.published_at
                .cmp(&right.published_at)
                .then(left.id.cmp(&right.id))
        })
}
