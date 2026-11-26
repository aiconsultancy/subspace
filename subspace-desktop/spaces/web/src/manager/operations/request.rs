use std::path::PathBuf;

use gpui_kit::App;
use subspace_framework::core::settings::SettingsFile;
use subspace_framework::dashboard::CurrentProduct;
use subspace_framework::product::{BoardIdUse, ValidatedProduct};

use super::super::model::{self, SourceKind};
use super::{Manager, Preparation, config_path};
use crate::install::{ImportRequest, RepositoryRef};

pub(super) fn from_editor(manager: &Manager, cx: &App) -> Result<Preparation, String> {
    let editor = manager.editor.as_ref().ok_or("Choose Add board first.")?;
    let (id, title) = editor.identity(cx);
    model::validate_identity(&id, &title)?;
    let file = SettingsFile::open(&config_path(cx)?).map_err(|error| error.to_string())?;
    let exists = file.web_board(&id).is_some();
    if exists != editor.replacing {
        return Err(if exists {
            "This board ID already exists. Edit that board or choose another ID."
        } else {
            "This board was removed. Add it again with a new confirmation."
        }
        .into());
    }
    if let Some(product) = cx.try_global::<CurrentProduct>() {
        built_in_free(&product.0.product, &id)?;
    }
    let request = source_request(editor, cx)?;
    Ok(Preparation {
        request,
        id,
        title,
        replacing: editor.replacing,
        file,
    })
}

/// Refuses `id` when one of the product's own boards uses it, as its ID or a
/// former ID, as the shell would ignore the board.
fn built_in_free(product: &ValidatedProduct, id: &str) -> Result<(), String> {
    match product.board_id_use(id) {
        None => Ok(()),
        Some(BoardIdUse::Id) => {
            Err("A built-in board already uses this ID. Choose another ID.".into())
        }
        Some(BoardIdUse::FormerId) => Err(
            "A built-in board used this ID before, and its saved settings are read as that board's. Choose another ID."
                .into(),
        ),
    }
}

fn source_request(
    editor: &super::super::editor::Editor,
    cx: &App,
) -> Result<ImportRequest, String> {
    let location = editor.location.read(cx).value().trim().to_owned();
    if location.is_empty() {
        return Err("Enter a source location.".into());
    }
    Ok(match editor.source_kind(cx) {
        SourceKind::Website => ImportRequest::Website { url: location },
        SourceKind::Folder => ImportRequest::Folder {
            path: PathBuf::from(location),
        },
        SourceKind::Zip => ImportRequest::Zip {
            path: PathBuf::from(location),
        },
        SourceKind::Repository => ImportRequest::Repository {
            repository: repository(&location)?,
            reference: reference(editor.reference.read(cx).value().trim())?,
            board_path: editor.board_path.read(cx).value().trim().to_owned(),
        },
        SourceKind::Release => {
            let selected = editor
                .releases
                .read(cx)
                .selected_value()
                .cloned()
                .ok_or("Load and select a published release first.")?;
            let (_, release_id, asset_name) = editor
                .release_ids
                .iter()
                .find(|(label, _, _)| label == &selected)
                .ok_or("Load releases again for this repository.")?;
            ImportRequest::Release {
                repository: repository(&location)?,
                release_id: *release_id,
                asset_name: asset_name.clone(),
                include_prereleases: editor.prereleases,
            }
        }
    })
}

pub(super) fn repository(text: &str) -> Result<String, String> {
    let repository = if text.starts_with("https://") {
        let url = url::Url::parse(text)
            .map_err(|_| "Enter owner/repository or its GitHub repository URL.")?;
        if url.host_str() != Some("github.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "Use a github.com repository URL without credentials, query or fragment.".into(),
            );
        }
        let path = url.path().trim_matches('/');
        if path.split('/').count() != 2 {
            return Err(
                "Use the repository root URL and enter its ref and board directory separately."
                    .into(),
            );
        }
        path.to_owned()
    } else {
        text.to_owned()
    };
    subspace_framework::client_github::validate::check_repo(&repository)
        .map_err(|error| error.to_string())?;
    Ok(repository)
}

fn reference(text: &str) -> Result<RepositoryRef, String> {
    if text.is_empty() {
        return Ok(RepositoryRef::DefaultBranch);
    }
    if let Some(name) = text.strip_prefix("tag:") {
        return Ok(RepositoryRef::Tag(name.to_owned()));
    }
    if let Some(name) = text.strip_prefix("commit:") {
        subspace_framework::client_github::validate::check_sha(name)
            .map_err(|error| error.to_string())?;
        return Ok(RepositoryRef::Commit(name.to_owned()));
    }
    Ok(RepositoryRef::Branch(
        text.strip_prefix("branch:").unwrap_or(text).to_owned(),
    ))
}

pub(super) fn is_github(request: &ImportRequest) -> bool {
    !matches!(
        request,
        ImportRequest::Website { .. } | ImportRequest::Folder { .. } | ImportRequest::Zip { .. }
    )
}

#[cfg(test)]
mod tests {
    use subspace_framework::product::{fixture, validate_product};

    use super::{built_in_free, reference, repository};
    use crate::install::RepositoryRef;

    #[test]
    fn a_board_id_or_former_id_of_the_product_is_refused() {
        let mut descriptor = fixture::descriptor();
        descriptor.boards[0].former_ids = vec!["example-old".into()];
        let product = validate_product(&descriptor).unwrap();
        let id = product.boards[0].id.as_str().to_owned();
        assert!(
            built_in_free(&product, &id)
                .unwrap_err()
                .contains("already uses")
        );
        let former = built_in_free(&product, "example-old").unwrap_err();
        assert!(former.contains("used this ID before"), "{former}");
        assert_eq!(built_in_free(&product, "example-weather"), Ok(()));
    }

    #[test]
    fn ambiguous_tree_urls_and_external_destinations_are_not_guessed() {
        assert_eq!(
            repository("https://github.com/test-org/test-board/").unwrap(),
            "test-org/test-board"
        );
        assert!(repository("https://github.com/test-org/test-board/tree/a/b").is_err());
        assert!(repository("https://elsewhere.example/test-org/test-board").is_err());
        assert!(repository("https://test-only-user@github.com/test-org/test-board").is_err());
    }

    #[test]
    fn explicit_pinned_refs_remain_pinned() {
        assert_eq!(reference("").unwrap(), RepositoryRef::DefaultBranch);
        assert_eq!(
            reference("feature/demo").unwrap(),
            RepositoryRef::Branch("feature/demo".into())
        );
        assert_eq!(
            reference("tag:v1").unwrap(),
            RepositoryRef::Tag("v1".into())
        );
        assert!(reference("commit:not-a-commit").is_err());
    }
}
