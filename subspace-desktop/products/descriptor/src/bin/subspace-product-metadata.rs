//! Packaging metadata without linking or initializing a GUI.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [id, version] = args.as_slice() else {
        eprintln!("usage: subspace-product-metadata <subspace> <version>");
        return ExitCode::from(2);
    };
    let descriptor = subspace_descriptor::known_product_descriptors()
        .into_iter()
        .find(|product| &product.id == id);
    let Some(mut descriptor) = descriptor else {
        eprintln!("unknown product: {id}");
        return ExitCode::from(2);
    };
    let metadata = descriptor
        .apply_default_theme_override(option_env!("SUBSPACE_DEFAULT_THEME"))
        .and_then(|()| subspace_framework::product::product_metadata(&descriptor, version));
    match metadata {
        Ok(metadata) => {
            println!("{metadata}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
