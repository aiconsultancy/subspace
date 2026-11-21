//! Fonts an app embeds and registers with the text system at startup, so a
//! theme that names them draws the same faces on every platform, and in every
//! screenshot, without the fonts being installed.
//!
//! A registered family counts as installed when a theme or Settings asks for
//! it by name. Automatic font choices skip the families only the app
//! provides, so themes that name no font keep the platform's fonts.

use std::borrow::Cow;

use gpui_kit::{App, Global};

/// One embedded font file and the family it registers.
///
/// `family` is the font's typographic family (OpenType name ID 16, or name
/// ID 1 when it has none): the name themes and Settings use, and the one the
/// text systems group the file's face under. Embed static files, one per
/// weight and style: the macOS text system registers a variable font as a
/// single face at its default weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BundledFont {
    pub family: &'static str,
    pub bytes: &'static [u8],
}

impl BundledFont {
    /// A font file for `family`, usually from `include_bytes!`.
    // qual:api (apps list their embedded files with it)
    pub const fn new(family: &'static str, bytes: &'static [u8]) -> Self {
        Self { family, bytes }
    }
}

/// The families the app registered, and those of them the platform did not
/// already provide.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct BundledFamilies {
    registered: Vec<String>,
    bundled_only: Vec<String>,
}

impl Global for BundledFamilies {}

/// Registers the app's fonts before the theme is applied. Returns a notice
/// when the platform refuses them; themes then fall back as they do for any
/// family that is not installed.
pub(crate) fn register(fonts: &'static [BundledFont], cx: &mut App) -> Option<String> {
    let platform = cx.text_system().all_font_names();
    let result = cx
        .text_system()
        .add_fonts(font_data(fonts))
        .map_err(|error| error.to_string());
    record(result, families(fonts, &platform), cx)
}

/// The families registered at startup. They count as installed.
pub(crate) fn registered(cx: &App) -> Vec<String> {
    stored(cx).registered
}

/// The registered families the platform did not provide, which automatic
/// font choices skip.
pub(crate) fn bundled_only(cx: &App) -> Vec<String> {
    stored(cx).bundled_only
}

fn stored(cx: &App) -> BundledFamilies {
    cx.try_global::<BundledFamilies>()
        .cloned()
        .unwrap_or_default()
}

/// `names` without the `skipped` families, compared without case.
pub(crate) fn without(names: &[String], skipped: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| !skipped.iter().any(|skip| skip.eq_ignore_ascii_case(name)))
        .cloned()
        .collect()
}

fn font_data(fonts: &'static [BundledFont]) -> Vec<Cow<'static, [u8]>> {
    fonts.iter().map(|font| Cow::Borrowed(font.bytes)).collect()
}

/// Each family once, in the order first listed, and which of them
/// `platform` (the families before registration) lacks.
fn families(fonts: &[BundledFont], platform: &[String]) -> BundledFamilies {
    let mut registered: Vec<String> = Vec::new();
    for font in fonts {
        if !registered
            .iter()
            .any(|family| family.as_str() == font.family)
        {
            registered.push(font.family.to_owned());
        }
    }
    let bundled_only = registered
        .iter()
        .filter(|family| {
            !platform
                .iter()
                .any(|name| name.eq_ignore_ascii_case(family))
        })
        .cloned()
        .collect();
    BundledFamilies {
        registered,
        bundled_only,
    }
}

fn record(result: Result<(), String>, families: BundledFamilies, cx: &mut App) -> Option<String> {
    match result {
        Ok(()) => {
            cx.set_global(families);
            None
        }
        Err(error) => Some(format!("The app's bundled fonts are unavailable: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;

    use super::{
        BundledFamilies, BundledFont, bundled_only, families, record, register, registered, without,
    };

    /// Fictional families: the test text system loads no font data.
    const FIXTURE: &[BundledFont] = &[
        BundledFont::new("Example Sans", b"regular"),
        BundledFont::new("Example Sans", b"bold"),
        BundledFont::new("Example Mono", b"mono"),
    ];

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn each_family_is_listed_once_and_platform_families_are_not_bundled_only() {
        let found = families(FIXTURE, &names(&["example mono", "Menlo"]));

        assert_eq!(found.registered, names(&["Example Sans", "Example Mono"]));
        assert_eq!(found.bundled_only, names(&["Example Sans"]));
        assert_eq!(
            families(&[], &names(&["Menlo"])),
            BundledFamilies::default()
        );
    }

    #[test]
    fn automatic_choices_skip_bundled_only_families_without_case() {
        let installed = names(&["Example Sans", "Menlo", ".SystemUIFont"]);

        assert_eq!(
            without(&installed, &names(&["example sans"])),
            names(&["Menlo", ".SystemUIFont"])
        );
        assert_eq!(without(&installed, &[]), installed);
    }

    #[gpui_kit::test]
    fn registered_families_count_as_installed(cx: &mut TestAppContext) {
        cx.update(|cx| {
            assert!(registered(cx).is_empty(), "nothing before registration");

            assert_eq!(register(FIXTURE, cx), None);

            assert_eq!(registered(cx), names(&["Example Sans", "Example Mono"]));
            assert_eq!(bundled_only(cx), names(&["Example Sans", "Example Mono"]));
        });
    }

    #[gpui_kit::test]
    fn a_refused_registration_reports_it_and_registers_nothing(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let notice = record(Err("refused".into()), families(FIXTURE, &[]), cx);

            assert!(notice.is_some_and(|notice| notice.contains("refused")));
            assert!(registered(cx).is_empty());
            assert!(bundled_only(cx).is_empty());
        });
    }
}
