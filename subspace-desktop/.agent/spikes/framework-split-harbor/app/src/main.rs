//! Harbor's whole composition: its identity, its Spaces and its artwork.
//! The Framework's shell owns startup, the window and the headless commands.

use std::process::ExitCode;

use subspace_framework::product::{BoardDescriptor, ProductDescriptor};
use subspace_framework::shell::{BoardRegistration, ProductBuild};

fn main() -> ExitCode {
    subspace_framework::shell::launch(
        ProductBuild {
            descriptor: descriptor(),
            default_theme_override: None,
            version: env!("CARGO_PKG_VERSION"),
            subspace_version: subspace_framework::product::SUBSPACE_VERSION,
            search_reranker: None,
            configured_boards: None,
            settings_extensions: &[],
        },
        spaces(),
        assets(),
        subspace_framework::board_command::run,
    )
}

/// Harbor's name, bundle id, data folders (from `binary_name`), themes and
/// Spaces in switcher order. No update source, so Harbor has no `upgrade`
/// command or update check.
fn descriptor() -> ProductDescriptor {
    ProductDescriptor {
        id: "harbor".into(),
        binary_name: "harbor".into(),
        display_name: "Harbor".into(),
        bundle_id: "com.example.harbor".into(),
        icon_paths: vec!["assets/harbor.svg".into()],
        themes: vec!["cupertino".into(), "one-dark".into()],
        default_theme: None,
        boards: vec![space("hello"), space("component-gallery")],
        update: None,
    }
}

fn space(id: &str) -> BoardDescriptor {
    BoardDescriptor {
        id: id.into(),
        aliases: Vec::new(),
        default_enabled: true,
    }
}

fn spaces() -> Vec<BoardRegistration> {
    vec![
        BoardRegistration::new::<harbor_hello_space::Hello>(),
        BoardRegistration::new::<subspace_framework::component_gallery::ComponentGallery>(),
    ]
}

fn assets() -> &'static [(&'static str, &'static [u8])] {
    &[("product.svg", include_bytes!("../assets/harbor.svg"))]
}

#[cfg(test)]
mod tests {
    use super::{assets, descriptor, spaces};
    use subspace_framework::shell::test_support::{AssetSource as _, Assets};

    #[test]
    fn the_descriptor_is_a_valid_product_with_harbors_identity() {
        let product = subspace_framework::product::validate_product(&descriptor()).unwrap();
        assert_eq!(product.binary_name.as_str(), "harbor");
        assert_eq!(product.display_name, "Harbor");
        assert_eq!(product.bundle_id, "com.example.harbor");
        assert!(product.update.is_none());
    }

    #[test]
    fn every_described_space_is_registered_in_the_same_order() {
        let described: Vec<_> = descriptor()
            .boards
            .into_iter()
            .map(|board| board.id)
            .collect();
        let registered: Vec<_> = spaces().iter().map(|space| space.id.to_owned()).collect();
        assert_eq!(described, registered);
    }

    #[test]
    fn the_app_serves_its_own_logo_and_each_spaces_artwork() {
        let registrations = spaces();
        let source = Assets::new(assets(), &registrations);
        assert!(source.load("product.svg").unwrap().is_some());
        assert!(source.load("icons/component.svg").unwrap().is_some());
    }
}
