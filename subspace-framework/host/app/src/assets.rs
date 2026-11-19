//! Embedded toolbar artwork, with GPUI Kit's assets kept as the fallback.

use std::borrow::Cow;

use gpui_kit::component::{ActiveTheme as _, ThemeMode};
use gpui_kit::{
    AnyElement, App, AssetSource, IntoElement as _, ObjectFit, Pixels, Result, SharedString,
    Styled as _, StyledImage as _, img, rgb, svg,
};

/// Preserve the artwork's baked brand tones when drawing its alpha mask.
const DARK_MARK_TONE: u32 = 0xF0EFED;
const LIGHT_MARK_TONE: u32 = 0x111111;

/// Resolve on every render. Distinct asset paths keep GPUI's image cache valid
/// across theme changes without changing the registered asset provider.
pub(crate) fn product_logo(cx: &App, size: Pixels) -> AnyElement {
    logo_element(
        product_logo_path(cx.asset_source().as_ref(), cx),
        cx.theme().mode,
        size,
    )
}

/// The optional product-supplied logo keeps its colours and proportions.
pub(crate) fn organization_logo(cx: &App, size: Pixels) -> Option<AnyElement> {
    const PATH: &str = "organization.svg";
    cx.asset_source().load(PATH).ok().flatten().map(|_| {
        img(PATH)
            .size(size)
            .object_fit(ObjectFit::Contain)
            .into_any_element()
    })
}

/// Rasterise themed marks at the slot's device-pixel size. Compatibility
/// artwork retains its own colours rather than becoming a one-tone mask.
fn logo_element(path: &'static str, mode: ThemeMode, size: Pixels) -> AnyElement {
    let tone = match (path, mode) {
        ("product.svg", _) => return img(path).size(size).into_any_element(),
        (_, ThemeMode::Dark) => DARK_MARK_TONE,
        (_, ThemeMode::Light) => LIGHT_MARK_TONE,
    };
    svg()
        .path(path)
        .size(size)
        .text_color(rgb(tone))
        .into_any_element()
}

fn product_logo_path(assets: &dyn AssetSource, cx: &App) -> &'static str {
    let path = match cx.theme().mode {
        ThemeMode::Dark => "product-dark.svg",
        ThemeMode::Light => "product-light.svg",
    };
    if matches!(assets.load(path), Ok(Some(_))) {
        path
    } else {
        "product.svg"
    }
}

pub struct Assets {
    artwork: Vec<(&'static str, &'static [u8])>,
}

impl Assets {
    /// Include every compiled board, even when disabled in Settings. Explicit
    /// artwork order preserves legacy catalogs without coupling it to tabs.
    pub fn new(
        product: &'static [(&'static str, &'static [u8])],
        boards: &[crate::BoardRegistration],
    ) -> Self {
        let mut declarations = boards
            .iter()
            .map(|board| board.artwork())
            .collect::<Vec<_>>();
        declarations.sort_by_key(|artwork| artwork.order());
        let artwork = product
            .iter()
            .copied()
            .chain(
                declarations
                    .iter()
                    .flat_map(|artwork| artwork.entries().iter().copied()),
            )
            .collect();
        Self { artwork }
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match self.artwork.iter().find(|(name, _)| *name == path) {
            Some((_, bytes)) => Ok(Some(Cow::Borrowed(bytes))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui_kit::assets::Assets.list(path)?;
        assets.extend(
            self.artwork
                .iter()
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| SharedString::from(*name)),
        );
        Ok(assets)
    }
}

#[cfg(test)]
mod tests {
    use super::{Assets, product_logo_path};
    use gpui_kit::component::{Theme, ThemeMode};
    use gpui_kit::{AssetSource as _, TestAppContext};

    #[gpui_kit::test]
    fn product_logo_tracks_theme_changes_with_distinct_cache_paths(cx: &mut TestAppContext) {
        const ARTWORK: &[(&str, &[u8])] = &[
            ("product.svg", b"compatibility"),
            ("product-dark.svg", b"dark logo"),
            ("product-light.svg", b"light logo"),
            ("product-light.svg", b"shadowed light logo"),
        ];
        let assets = Assets::new(ARTWORK, &[]);
        cx.update(|cx| {
            gpui_kit::init(cx);
            for (mode, expected_path, expected_bytes) in [
                (ThemeMode::Dark, "product-dark.svg", b"dark logo".as_slice()),
                (
                    ThemeMode::Light,
                    "product-light.svg",
                    b"light logo".as_slice(),
                ),
                (ThemeMode::Dark, "product-dark.svg", b"dark logo".as_slice()),
            ] {
                Theme::change(mode, None, cx);
                let path = product_logo_path(&assets, cx);
                assert_eq!(path, expected_path);
                assert_eq!(assets.load(path).unwrap().unwrap().as_ref(), expected_bytes);
            }
        });
    }

    #[gpui_kit::test]
    fn product_logo_falls_back_when_the_active_variant_is_not_registered(cx: &mut TestAppContext) {
        const UNTHEMED: &[(&str, &[u8])] = &[("product.svg", b"unchanged product")];
        const DARK_ONLY: &[(&str, &[u8])] = &[
            ("product.svg", b"compatibility"),
            ("product-dark.svg", b"dark logo"),
        ];
        cx.update(|cx| {
            gpui_kit::init(cx);
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                Theme::change(mode, None, cx);
                let assets = Assets::new(UNTHEMED, &[]);
                let path = product_logo_path(&assets, cx);
                assert_eq!(path, "product.svg");
                assert_eq!(
                    assets.load(path).unwrap().unwrap().as_ref(),
                    b"unchanged product"
                );
            }
            Theme::change(ThemeMode::Light, None, cx);
            assert_eq!(
                product_logo_path(&Assets::new(DARK_ONLY, &[]), cx),
                "product.svg"
            );
        });
    }

    #[test]
    fn toolbar_artwork_is_embedded_and_kit_icons_still_load() {
        const ARTWORK: &[(&str, &[u8])] = &[("host/fixture.svg", b"host artwork")];
        for (path, bytes) in ARTWORK {
            assert_eq!(
                Assets::new(ARTWORK, &[])
                    .load(path)
                    .unwrap()
                    .unwrap()
                    .as_ref(),
                *bytes
            );
        }
        assert!(
            Assets::new(&[], &[])
                .load("icons/eye.svg")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn product_first_match_wins_and_lists_keep_duplicates_before_missing_fallback() {
        const OVERRIDES: &[(&str, &[u8])] = &[
            ("logos/azure.svg", b"first"),
            ("logos/azure.svg", b"second"),
            ("icons/eye.svg", b"custom eye"),
        ];
        let assets = Assets::new(OVERRIDES, &[]);
        assert_eq!(
            assets.load("logos/azure.svg").unwrap().unwrap().as_ref(),
            b"first"
        );
        assert_eq!(
            assets.load("icons/eye.svg").unwrap().unwrap().as_ref(),
            b"custom eye"
        );
        assert!(assets.load("missing/step11.svg").is_err());
        let kit = gpui_kit::assets::Assets.list("icons/").unwrap();
        let icons = assets.list("icons/").unwrap();
        assert_eq!(&icons[..kit.len()], kit.as_slice());
        assert_eq!(icons.last().unwrap().as_ref(), "icons/eye.svg");
        let logos = assets.list("logos/").unwrap();
        assert_eq!(&logos[..2], &["logos/azure.svg", "logos/azure.svg"]);
        assert_eq!(logos.len(), 2);
    }
}
