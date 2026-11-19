use serde_json::Value;
use subspace_core::http::{Error, status};

use super::{Organization, PackageRelease, PackageRepository, ReleaseAsset, RepositoryRef};
use crate::{transport, validate};

const MAX_REF_BYTES: usize = 256;

pub(super) fn organization(row: Value) -> Result<Organization, Error> {
    let login = row["organization"]["login"]
        .as_str()
        .ok_or_else(|| transport::unexpected("organization"))?;
    membership(&row, login)?;
    validate::check_login(login)?;
    Ok(Organization {
        login: login.to_owned(),
    })
}

pub(super) fn membership(row: &Value, organization: &str) -> Result<(), Error> {
    let matched = row["organization"]["login"]
        .as_str()
        .is_some_and(|login| login.eq_ignore_ascii_case(organization));
    if row["state"] != "active" || !matched {
        return Err(transport::status(
            status::FORBIDDEN,
            "This account has no verified active organization membership. Sign in with an eligible account.",
        ));
    }
    Ok(())
}

pub(super) fn repository(value: Value, requested: &str) -> Result<PackageRepository, Error> {
    let matching = value["full_name"]
        .as_str()
        .is_some_and(|name| name.eq_ignore_ascii_case(requested));
    if value["owner"]["type"] != "Organization" || value["permissions"]["pull"] != true || !matching
    {
        return Err(transport::status(
            status::FORBIDDEN,
            "This account has no verified read access to this organization repository.",
        ));
    }
    serde_json::from_value(value).map_err(|_| transport::unexpected("repository"))
}

pub(super) fn effective_ref(
    reference: &RepositoryRef,
    default: &str,
) -> Result<RepositoryRef, Error> {
    let effective = match reference {
        RepositoryRef::DefaultBranch => RepositoryRef::Branch(default.into()),
        reference => reference.clone(),
    };
    expression(&effective)?;
    Ok(effective)
}

pub(super) fn expression(reference: &RepositoryRef) -> Result<String, Error> {
    match reference {
        RepositoryRef::Commit(commit) => validate::check_sha(commit).map(str::to_owned),
        RepositoryRef::Branch(name) | RepositoryRef::Tag(name) => {
            check_ref_text(name)?;
            check_ref_parts(name)?;
            let namespace = if matches!(reference, RepositoryRef::Branch(_)) {
                "heads"
            } else {
                "tags"
            };
            Ok(format!("refs/{namespace}/{name}"))
        }
        RepositoryRef::DefaultBranch => Err(validate::invalid(
            "Resolve the repository default branch first.",
        )),
    }
}

fn check_ref_text(name: &str) -> Result<(), Error> {
    if name.is_empty()
        || name.len() > MAX_REF_BYTES
        || name.chars().any(char::is_control)
        || name.contains(['?', '#', '\\', ':', ' ', '~', '^', '*', '['])
        || name.contains("..")
        || name.contains("@{")
        || name.ends_with('.')
    {
        return Err(validate::invalid(
            "Select an explicit branch, tag or full commit SHA, not a revision expression.",
        ));
    }
    Ok(())
}

fn check_ref_parts(name: &str) -> Result<(), Error> {
    if name
        .split('/')
        .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
    {
        return Err(validate::invalid("Select a valid branch or tag name."));
    }
    Ok(())
}

pub(super) fn asset<'a>(
    release: &'a PackageRelease,
    name: &str,
    prereleases: bool,
) -> Result<&'a ReleaseAsset, Error> {
    if release.draft
        || release.published_at.is_none()
        || (release.prerelease && !prereleases)
        || !name.to_ascii_lowercase().ends_with(".zip")
        || name.contains(['/', '\\'])
    {
        return Err(validate::invalid(
            "Select a published release with an eligible named ZIP asset.",
        ));
    }
    let mut matching = release
        .assets
        .iter()
        .filter(|asset| asset.name == name && asset.id != 0);
    let asset = matching.next().ok_or_else(|| {
        validate::invalid("This release does not contain the exact selected ZIP asset.")
    })?;
    if matching.next().is_some() {
        return Err(validate::invalid("The release has ambiguous asset names."));
    }
    Ok(asset)
}
