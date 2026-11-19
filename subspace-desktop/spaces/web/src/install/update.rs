use super::{ImportRequest, ManagedSource, RepositoryRef};
use subspace_framework::client_github::{BoundRead, newest_package_release};

/// An offer does not download, prepare files or mutate an installation.
#[derive(Clone)]
pub struct UpdateOffer {
    request: ImportRequest,
    installed: String,
    available: String,
}

impl UpdateOffer {
    fn new(request: ImportRequest, installed: String, available: String) -> Self {
        Self {
            request,
            installed,
            available,
        }
    }
    pub fn request(&self) -> ImportRequest {
        self.request.clone()
    }
    pub fn installed(&self) -> &str {
        &self.installed
    }
    pub fn available(&self) -> &str {
        &self.available
    }
    pub fn label(&self) -> String {
        format!("Update from {} to {}", self.installed, self.available)
    }
}

/// Caller must discard this result after any selected-account or board revision
/// change. Downloads separately reauthorize when an offer is accepted.
pub fn check_update(
    source: &ManagedSource,
    bound: &BoundRead,
) -> Result<Option<UpdateOffer>, String> {
    match source {
        ManagedSource::Repository {
            repository,
            reference,
            board_path,
            commit,
            ..
        } => repository_update(bound, repository, reference, board_path, commit),
        ManagedSource::Release {
            repository,
            release_id,
            tag,
            asset_name,
            include_prereleases,
            ..
        } => {
            let releases = bound
                .package_releases(repository)
                .map_err(|error| error.to_string())?;
            Ok(
                newest_package_release(&releases, asset_name, *include_prereleases)
                    .filter(|release| release.id != *release_id)
                    .filter(|release| is_newer(&releases, *release_id, release.id))
                    .map(|release| {
                        UpdateOffer::new(
                            ImportRequest::Release {
                                repository: repository.clone(),
                                release_id: release.id,
                                asset_name: asset_name.clone(),
                                include_prereleases: *include_prereleases,
                            },
                            tag.clone(),
                            release.tag_name.clone(),
                        )
                    }),
            )
        }
        _ => Ok(None),
    }
}

fn repository_update(
    bound: &BoundRead,
    repository: &str,
    reference: &RepositoryRef,
    board_path: &str,
    commit: &str,
) -> Result<Option<UpdateOffer>, String> {
    let resolved = bound
        .resolve_repository(repository, reference)
        .map_err(|error| error.to_string())?;
    let moving = matches!(
        reference,
        RepositoryRef::Branch(_) | RepositoryRef::DefaultBranch
    );
    Ok((moving && resolved.commit != commit).then(|| {
        UpdateOffer::new(
            ImportRequest::RepositorySnapshot {
                repository: repository.into(),
                reference: reference.clone(),
                commit: resolved.commit.clone(),
                board_path: board_path.into(),
            },
            commit.into(),
            resolved.commit,
        )
    }))
}

fn is_newer(
    releases: &[subspace_framework::client_github::PackageRelease],
    installed: u64,
    available: u64,
) -> bool {
    let old = releases.iter().find(|release| release.id == installed);
    let new = releases.iter().find(|release| release.id == available);
    match (old, new) {
        (Some(old), Some(new)) => new.published_at > old.published_at,
        (None, Some(_)) => available > installed,
        _ => false,
    }
}
