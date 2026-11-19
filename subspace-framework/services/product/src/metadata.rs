//! Validated product metadata for packaging and headless binary inspection.

use serde_json::Value;

use crate::{Channel, ProductDescriptor, ProductError, asset_stem, validate_product};

/// The product as packaging sees it. `channel` is this build's channel and
/// `asset_stem` names the macOS app archive; the bundle directory inside it
/// is `<display_name>.app`. `version` is the app's; `subspace_version` is
/// this Framework's.
pub fn product_metadata(
    descriptor: &ProductDescriptor,
    version: &str,
) -> Result<Value, ProductError> {
    let product = validate_product(descriptor)?;
    Ok(serde_json::json!({
        "id": product.id.as_str(), "binary_name": product.binary_name.as_str(),
        "display_name": product.display_name, "bundle_id": product.bundle_id,
        "release_repository": product.release_repository,
        "channel": Channel::current().as_str(),
        "asset_stem": asset_stem(&product.display_name),
        "icon_paths": product.icon_paths, "credits": product.credits, "version": version,
        "subspace_version": crate::SUBSPACE_VERSION,
        "themes": product.themes.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        "default_theme": product.default_theme.as_str(),
        "boards": product.boards.iter().map(|board| board.id.as_str()).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use crate::Channel;

    #[test]
    fn product_manifests_match_their_validated_descriptors() {
        let descriptor = crate::fixture::descriptor();
        let metadata = super::product_metadata(&descriptor, "1.1.0").unwrap();
        assert_eq!(metadata["binary_name"], descriptor.binary_name);
        assert_eq!(metadata["display_name"], descriptor.display_name);
        assert_eq!(
            metadata["boards"].as_array().unwrap().len(),
            descriptor.boards.len()
        );
        assert_eq!(
            metadata["default_theme"],
            descriptor.default_theme.as_deref().unwrap()
        );
        assert_eq!(metadata["version"], "1.1.0");
        assert_eq!(metadata["subspace_version"], crate::SUBSPACE_VERSION);
        assert_eq!(metadata["channel"], Channel::current().as_str());
        assert_eq!(
            metadata["asset_stem"],
            descriptor.update.as_ref().unwrap().app_bundle
        );
    }

    #[test]
    fn a_type_build_names_its_bundle_and_archive_after_the_type() {
        for (descriptor, stem) in [
            (
                crate::fixture::descriptor_for(Channel::Nightly),
                "Subspace-Nightly",
            ),
            (
                crate::fixture::descriptor_for(Channel::Beta),
                "Subspace-Beta",
            ),
        ] {
            let metadata = super::product_metadata(&descriptor, "1.0.0").unwrap();
            assert_eq!(metadata["asset_stem"], stem);
            assert_eq!(metadata["display_name"], descriptor.display_name);
            assert_eq!(metadata["bundle_id"], descriptor.bundle_id);
        }
    }
}
