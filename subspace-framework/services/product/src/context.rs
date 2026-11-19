//! Product context consumed by the shell and the headless commands.

use crate::build::BuildInfo;
use crate::descriptor::ValidatedProduct;
use crate::paths::{ConfigRoots, product_config_roots, product_data_name};
use crate::platform::PlatformEnv;

/// Validated current product, its build and injectable platform roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductContext {
    pub product: ValidatedProduct,
    pub version: String,
    pub subspace_version: String,
    pub build: BuildInfo,
    pub env: PlatformEnv,
}

impl ProductContext {
    pub fn new(product: ValidatedProduct, env: PlatformEnv) -> Self {
        Self {
            product,
            version: crate::SUBSPACE_VERSION.to_owned(),
            subspace_version: crate::SUBSPACE_VERSION.to_owned(),
            build: BuildInfo::UNSTAMPED,
            env,
        }
    }

    pub fn product_roots(&self) -> ConfigRoots {
        product_config_roots(
            &self.env,
            &product_data_name(self.product.binary_name.as_str()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ProductContext;
    use crate::fixture;
    use crate::platform::{OsKind, PlatformEnv};
    use std::path::PathBuf;

    #[test]
    fn the_product_roots_follow_the_binary_name() {
        let known = fixture::products().unwrap();
        let env = PlatformEnv {
            os: OsKind::Macos,
            home: PathBuf::from("/no-such-home/fixture"),
            xdg_config_home: None,
            xdg_state_home: None,
            app_data: None,
            local_app_data: None,
        };
        let context = ProductContext::new(known[0].clone(), env);
        assert_eq!(
            context.product_roots().cross_platform,
            PathBuf::from("/no-such-home/fixture/.config")
                .join(crate::product_data_name("subspace"))
        );
        assert_eq!(context.version, crate::SUBSPACE_VERSION);
    }
}
